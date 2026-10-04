//! Índice de biblioteca en SQLite. Ver plan §5.4, guión A §7 y guión C (edición + procedimientos).

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Abre (o crea) la BD y asegura el esquema + migraciones.
pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

fn column_exists(conn: &Connection, table: &str, col: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let name: String = r.get(1)?;
        if name.eq_ignore_ascii_case(col) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Crea tablas (si faltan) y migra BDs antiguas. Reutilizable en tests (BD en memoria).
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS source(
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            path TEXT NOT NULL,
            default_category_id INTEGER,
            enabled INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS category(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            grp TEXT
        );
        CREATE TABLE IF NOT EXISTS game(
            id INTEGER PRIMARY KEY,
            source_id INTEGER,
            title TEXT NOT NULL,
            source_key TEXT,
            install_dir TEXT,
            exe_path TEXT,
            platform TEXT NOT NULL DEFAULT 'PC',
            cover_path TEXT,
            client_path TEXT,
            client_args TEXT,
            client_first INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS game_category(
            game_id INTEGER NOT NULL,
            category_id INTEGER NOT NULL,
            PRIMARY KEY (game_id, category_id)
        );
        CREATE TABLE IF NOT EXISTS executable(
            id INTEGER PRIMARY KEY,
            game_id INTEGER NOT NULL,
            label TEXT NOT NULL,
            path TEXT NOT NULL,
            args TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS launch_log(
            id INTEGER PRIMARY KEY,
            game_id INTEGER NOT NULL,
            executable_id INTEGER,
            exe_path TEXT NOT NULL,
            launched_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_launch_log_game ON launch_log(game_id, launched_at);
        -- Categorías que se aplican a TODA una fuente (F10). Sustituye a
        -- `source.default_category_id`, que solo admitía una.
        CREATE TABLE IF NOT EXISTS source_category(
            source_id   INTEGER NOT NULL,
            category_id INTEGER NOT NULL,
            PRIMARY KEY (source_id, category_id)
        );
        -- Categorías que el usuario le ha **quitado** a un juego concreto (guión A.0_8).
        -- Sin esto, la regla de la fuente se la volvía a poner en el siguiente escaneo: una
        -- carpeta entera en «Coop» no dejaba sacar de ahí el único juego que no lo es.
        CREATE TABLE IF NOT EXISTS categoria_rechazada(
            game_id     INTEGER NOT NULL,
            category_id INTEGER NOT NULL,
            PRIMARY KEY (game_id, category_id)
        );
        CREATE TABLE IF NOT EXISTS setting(
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );
        -- Espejo/caché de lo leído de la API de TimeTrack. Aquí NO se trackea (plan §4.5).
        CREATE TABLE IF NOT EXISTS playtime_cache(
            game_id    INTEGER PRIMARY KEY,
            exe_name   TEXT,
            total_secs INTEGER NOT NULL DEFAULT 0,
            last_sync  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;

    // Migración de BDs creadas antes de guión C / C.0_2 (columnas nuevas en `game`).
    for (col, decl) in [
        ("source_key", "source_key TEXT"),
        ("client_path", "client_path TEXT"),
        ("client_args", "client_args TEXT"),
        ("client_first", "client_first INTEGER NOT NULL DEFAULT 0"),
        ("exe_locked", "exe_locked INTEGER NOT NULL DEFAULT 0"),
        // C.0_2: args del ejecutable principal y marca de "ya no está en disco".
        ("exe_args", "exe_args TEXT"),
        ("missing", "missing INTEGER NOT NULL DEFAULT 0"),
        // Ronda 1 (L1): estado del elemento. Ver guión L.0_1.
        ("state", "state TEXT NOT NULL DEFAULT 'installed'"),
        // Ronda 1 (C1): modo de lanzamiento recordado ('game' | 'client'); NULL = preguntar.
        ("launch_mode", "launch_mode TEXT"),
        // Ronda 1 (F1/A3): nº de jugadores, para poder filtrar por él.
        ("players_min", "players_min INTEGER"),
        ("players_max", "players_max INTEGER"),
        // Ronda 3 (D1): arte traído de TimeTrack. La carátula local (`cover_path`) manda.
        ("art_path", "art_path TEXT"),
        // Ronda 4 (B1/B3/B4): origen de tienda y su id (appid de Steam), y la cuenta
        // concreta que hace falta para jugarlo. Ver guión B.0_1.
        ("store", "store TEXT"),
        ("store_id", "store_id TEXT"),
        ("steam_account", "steam_account TEXT"),
        // Favoritos: un juego puede ser favorito esté en el estado que esté.
        ("favorite", "favorite INTEGER NOT NULL DEFAULT 0"),
        // Cuándo entró en la biblioteca (guión F.0_6). **NULL en lo que ya existía**: de esos
        // no consta, y rellenarlos con la fecha de la migración diría que se añadieron todos
        // hoy — que es justo lo contrario de lo que se quiere saber.
        ("added_at", "added_at TEXT"),
        // La carátula la ha elegido **el usuario**: ni el escaneo ni la cascada la tocan.
        // Es el mismo mecanismo que `exe_locked`, y por la misma razón (guión D.0_4).
        ("cover_locked", "cover_locked INTEGER NOT NULL DEFAULT 0"),
        // El título lo ha cambiado el usuario: el escaneo no vuelve a poner el de la carpeta.
        ("title_locked", "title_locked INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !column_exists(conn, "game", col)? {
            conn.execute_batch(&format!("ALTER TABLE game ADD COLUMN {};", decl))?;
        }
    }

    // Id de la app en TimeTrack, para poder abrirle sus gráficas (guión T.0_2). Va en la caché
    // y no en `game` porque es un dato **de la integración**: si se cambia de perfil o se deja
    // de usar TimeTrack, se va con el resto de lo sincronizado.
    if !column_exists(conn, "playtime_cache", "tt_app_id")? {
        conn.execute_batch("ALTER TABLE playtime_cache ADD COLUMN tt_app_id INTEGER;")?;
    }

    reparar_caratulas_ascendidas(conn)?;

    // Fuentes duplicadas: se fusionan y, a partir de ahí, el índice único impide que vuelvan.
    // El índice va **después** de fusionar, porque con duplicados presentes no se puede crear.
    fusionar_fuentes_duplicadas(conn)?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_source_unica
             ON source (kind, LOWER(RTRIM(REPLACE(path, '/', '\\'), '\\')));",
    )?;

    // ── Categorías como árbol (guión F.0_6 / ADR-005) ───────────────────────────────
    for (col, decl) in [
        ("parent_id", "parent_id INTEGER"),
        ("sort_order", "sort_order INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !column_exists(conn, "category", col)? {
            conn.execute_batch(&format!("ALTER TABLE category ADD COLUMN {};", decl))?;
        }
    }
    // Los «grupos» de F.0_2 eran una etiqueta de texto (`grp`): no se podían ordenar, ni
    // renombrar sin tocar todas sus categorías, ni anidar. Pasan a ser **filas padre**.
    if column_exists(conn, "category", "grp")? {
        let grupos: Vec<String> = {
            let mut st = conn.prepare(
                "SELECT DISTINCT grp FROM category
                 WHERE grp IS NOT NULL AND TRIM(grp) <> '' ORDER BY grp",
            )?;
            let filas = st.query_map([], |r| r.get::<_, String>(0))?;
            filas.collect::<Result<_>>()?
        };
        for g in &grupos {
            // El nombre es UNIQUE: si ya existía una categoría con el nombre del grupo, se
            // reutiliza esa fila como padre en vez de duplicarla.
            conn.execute("INSERT OR IGNORE INTO category (name) VALUES (?1)", params![g])?;
        }
        conn.execute(
            "UPDATE category
                SET parent_id = (SELECT p.id FROM category p WHERE p.name = category.grp)
              WHERE grp IS NOT NULL AND TRIM(grp) <> ''
                AND parent_id IS NULL
                AND name <> grp",
            [],
        )?;
        // `grp` queda obsoleta: una sola fuente de verdad. Si el motor no soporta DROP
        // COLUMN, se neutraliza — el mismo patrón que se usó con `missing`.
        if conn
            .execute_batch("ALTER TABLE category DROP COLUMN grp;")
            .is_err()
        {
            conn.execute_batch("UPDATE category SET grp = NULL;")?;
        }
        ordenar_alfabeticamente(conn)?;
    }

    // Migración `missing` → `state` (L1). `missing` queda obsoleta: se vuelca una vez y se
    // elimina, para que no pueda quedar desincronizada y revertir estados en el siguiente
    // arranque. Si el motor no soporta DROP COLUMN, se neutraliza poniéndola a 0.
    if column_exists(conn, "game", "missing")? {
        conn.execute_batch(
            "UPDATE game SET state = 'uninstalled' WHERE missing = 1 AND state = 'installed';",
        )?;
        if conn
            .execute_batch("ALTER TABLE game DROP COLUMN missing;")
            .is_err()
        {
            conn.execute_batch("UPDATE game SET missing = 0;")?;
        }
    }
    // «Deseado» se unifica con «No instalado»: eran lo mismo visto por el usuario, y el
    // nombre se confundía con los Favoritos (guión L.0_2).
    conn.execute_batch(
        "UPDATE game SET state = 'uninstalled' WHERE state = 'wishlist';
         CREATE INDEX IF NOT EXISTS idx_game_state ON game(state);",
    )?;

    // F11: una categoría puede exigir confirmación antes de lanzar (caso «Launcher SpaceWar»).
    if !column_exists(conn, "category", "launch_prompt")? {
        conn.execute_batch("ALTER TABLE category ADD COLUMN launch_prompt TEXT;")?;
    }
    // F10: la categoría por defecto de cada fuente pasa a la tabla N:M, que admite varias.
    conn.execute_batch(
        "INSERT OR IGNORE INTO source_category (source_id, category_id)
         SELECT id, default_category_id FROM source WHERE default_category_id IS NOT NULL;",
    )?;

    // Backfill de source_key (clave de dedupe) y su índice.
    conn.execute_batch(
        "UPDATE game SET source_key = (
             SELECT CASE s.kind WHEN 'app_entry' THEN game.exe_path ELSE game.install_dir END
             FROM source s WHERE s.id = game.source_id
         ) WHERE source_key IS NULL OR source_key = '';
         UPDATE game SET source_key = COALESCE(exe_path, install_dir, CAST(id AS TEXT))
             WHERE source_key IS NULL OR source_key = '';
         CREATE INDEX IF NOT EXISTS idx_game_source_key ON game(source_key);",
    )?;
    Ok(())
}

#[derive(Serialize)]
pub struct Source {
    pub id: i64,
    pub kind: String,
    pub path: String,
    pub default_category_id: Option<i64>,
    pub enabled: bool,
    /// Juegos que provienen de esta fuente (para la UI de gestión).
    pub game_count: i64,
}

/// Una categoría del árbol (guión F.0_6). Los «grupos» de F.0_2 eran texto suelto en
/// `category.grp`; ahora son **categorías padre de verdad**, con `parent_id = NULL`.
#[derive(Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    /// `None` = es un **eje** (raíz): *Clientes*, *Nº Jugadores*, *TAGS*…
    pub parent_id: Option<i64>,
    /// Posición entre sus hermanos. Arranca alfabético (A2) y se cambia a mano (A3).
    pub sort_order: i64,
    /// Juegos **suyos y de sus descendientes**, sin repetir y sin contar los excluidos
    /// (ADR-005 §4.1: un juego en una hija cuenta también en su padre).
    pub count: i64,
    /// Si está, se pide esta confirmación antes de lanzar los juegos de la categoría (C8).
    pub launch_prompt: Option<String>,
}

/// Estados de un elemento de la biblioteca (guión L.0_1 §1).
pub const STATE_INSTALLED: &str = "installed";
pub const STATE_UNINSTALLED: &str = "uninstalled";
pub const STATE_EXCLUDED: &str = "excluded";

/// ¿Es un estado válido? Evita que la UI escriba cualquier cosa en `game.state`.
/// `wishlist` ya no existe: se unificó con `uninstalled` (guión L.0_2).
pub fn is_valid_state(s: &str) -> bool {
    matches!(s, STATE_INSTALLED | STATE_UNINSTALLED | STATE_EXCLUDED)
}

#[derive(Serialize)]
pub struct GameCard {
    pub id: i64,
    pub title: String,
    pub platform: String,
    pub cover_path: Option<String>,
    /// `installed` | `uninstalled` | `excluded`.
    pub state: String,
    /// `true` si no tiene ejecutable asignado (no se puede lanzar hasta editarlo).
    pub needs_exe: bool,
    pub players_min: Option<i64>,
    pub players_max: Option<i64>,
    /// Tiempo jugado (segundos) leído de TimeTrack. `0` si no hay dato.
    pub playtime_secs: i64,
    /// Appid de tienda: la UI lo usa para el CDN de Steam si no hay carátula local (D7).
    pub store_id: Option<String>,
    pub favorite: bool,
    /// Categorías a las que pertenece. Viaja en la card porque la cuadrícula **se parte en
    /// secciones** en el cliente (guión F.0_5) y necesita saberlo sin una consulta por juego.
    pub categories: Vec<i64>,
    /// `kind` de la fuente (`steam` · `folder_library` · `app_entry`), para seccionar por origen.
    pub origin: Option<String>,
    /// Tiene **alguna ruta registrada** (carpeta o ejecutable), así que se puede intentar abrir
    /// en el explorador. No dice que siga existiendo en disco: eso se comprueba al abrirla, que
    /// es cuando se puede dar un error con la ruta concreta. Sirve para no ofrecer la opción en
    /// juegos de tienda sin instalar, donde no hay nada que abrir.
    pub has_dir: bool,
}

/// Filtros combinables del grid (guiones F.0_1 y F.0_3). Todos opcionales y acumulativos.
///
/// **Estado y categoría son dimensiones independientes**: se pueden combinar (p. ej. los
/// excluidos *de* Steam) y la UI debe reflejar las dos como activas.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct LibraryFilter {
    pub category_id: Option<i64>,
    pub query: Option<String>,
    /// Estado exacto. Si es `None`, se muestran todos **menos** los excluidos.
    pub state: Option<String>,
    pub platform: Option<String>,
    /// Admite al menos N jugadores (`players_max >= N`).
    pub players: Option<i64>,
    /// `true` = solo los que no tienen ejecutable asignado (vista de mantenimiento).
    pub needs_exe: Option<bool>,
    /// Origen: `steam` · `folder_library` · `app_entry` (el `kind` de su fuente).
    pub origin: Option<String>,
    /// `true` = solo los que tienen tiempo jugado; `false` = solo los que nunca se jugaron.
    pub played: Option<bool>,
    /// Horas totales **como mínimo** (inclusive). Se combina con `played` y con `max_hours`, así
    /// que la UI puede pedir bandas («entre 5 y 20 h») sin que aquí haya que saber de bandas.
    pub min_hours: Option<f64>,
    /// Horas totales **como máximo**, exclusivo. Exclusivo y no inclusive para que bandas
    /// contiguas no se solapen: «1–5» y «5–20» no deben devolver dos veces el de 5 h justas.
    pub max_hours: Option<f64>,
    /// Criterio de orden: `title` (por defecto) · `recent` · `playtime` · `players` · `added`.
    /// La **dirección** va aparte, en `desc`: antes iban pegados (`playtime` ya significaba
    /// «más jugados primero») y no había forma de pedir el orden inverso.
    pub sort: Option<String>,
    /// `true` = descendente. Si no se dice nada, manda la dirección natural del criterio.
    pub desc: Option<bool>,
    /// `true` = solo favoritos.
    pub favorite: Option<bool>,
    /// Solo los añadidos en los últimos N días. Los que no tienen fecha (`added_at` nulo)
    /// **quedan fuera**: de esos no consta cuándo entraron, y colarlos sería inventar.
    pub added_days: Option<i64>,
}

/// Nº de elementos por estado (contadores de la sidebar).
#[derive(Serialize, Default)]
pub struct StateCounts {
    pub installed: i64,
    pub uninstalled: i64,
    pub excluded: i64,
    /// Marcados como favoritos, en cualquier estado.
    pub favorites: i64,
    /// Añadidos dentro de la ventana de «recientes» (la que diga la UI).
    pub recent: i64,
    /// Elementos sin ejecutable ni id de tienda: no se pueden lanzar hasta arreglarlos.
    pub needs_exe: i64,
}

#[derive(Serialize)]
pub struct Executable {
    pub id: i64,
    pub label: String,
    pub path: String,
    pub args: Option<String>,
}

#[derive(Serialize)]
pub struct GameDetail {
    pub id: i64,
    pub title: String,
    pub install_dir: Option<String>,
    pub exe_path: Option<String>,
    pub exe_args: Option<String>,
    pub platform: String,
    /// **Solo la carátula puesta a mano.** Antes aquí venía `COALESCE(cover_path, art_path)`, y
    /// eso era el origen de un fallo feo: el editor cargaba la automática en su campo y al
    /// pulsar *Guardar* la **ascendía a manual**, con lo que «Rehacer» ya no podía tocarla.
    pub cover_path: Option<String>,
    /// La resuelta por la cascada. Viaja aparte para poder enseñar las dos cosas distintas.
    pub art_path: Option<String>,
    pub client_path: Option<String>,
    pub client_args: Option<String>,
    pub client_first: bool,
    /// `installed` | `uninstalled` | `excluded`.
    pub state: String,
    /// Modo de lanzamiento recordado (`game` | `client`); `None` = preguntar cada vez.
    pub launch_mode: Option<String>,
    pub players_min: Option<i64>,
    pub players_max: Option<i64>,
    /// Tienda de origen (`steam`…), su id (appid) y la cuenta que requiere el juego.
    pub store: Option<String>,
    pub store_id: Option<String>,
    pub steam_account: Option<String>,
    /// Comprobación en vivo: el `exe_path` existe ahora mismo.
    pub exe_exists: bool,
    /// Registro de lanzamientos (base para la correlación con TimeTrack en M2).
    pub launch_count: i64,
    pub last_launched: Option<String>,
    /// Tiempo jugado (segundos) según TimeTrack, y cuándo se sincronizó.
    pub playtime_secs: i64,
    pub playtime_sync: Option<String>,
    /// Id de este juego **en TimeTrack**, si la sincronización lo encontró. `None` = no se puede
    /// ofrecer el botón de gráficas, porque no hay nada concreto que abrir.
    pub tt_app_id: Option<i64>,
    pub categories: Vec<String>,
    pub executables: Vec<Executable>,
    /// Marcado como favorito. Es **independiente del estado**: un juego del histórico o
    /// pendiente de instalar puede seguir siendo favorito (guión L.0_2 §2).
    pub favorite: bool,
}

/// Qué se lanza al pulsar LANZAR (guión C.0_3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Solo el juego. **No** se toca el launcher (los juegos de Steam ya lo levantan solos).
    GameOnly,
    /// Solo el launcher/cliente. **No** se lanza el juego.
    ClientOnly,
    /// Comportamiento clásico: cliente y después juego. Solo si no hay que preguntar.
    Auto,
}

impl LaunchMode {
    pub fn from_str(s: Option<&str>) -> Self {
        match s {
            Some("game") => LaunchMode::GameOnly,
            Some("client") => LaunchMode::ClientOnly,
            _ => LaunchMode::Auto,
        }
    }
    pub fn as_str(&self) -> Option<&'static str> {
        match self {
            LaunchMode::GameOnly => Some("game"),
            LaunchMode::ClientOnly => Some("client"),
            LaunchMode::Auto => None,
        }
    }
}

/// Lo que la UI necesita para decidir si enseña el popup de lanzamiento (C.0_3 §2).
#[derive(Serialize)]
pub struct LaunchOptions {
    pub title: String,
    /// Hay launcher/cliente configurado **y** no hay modo recordado → hay que preguntar.
    pub needs_prompt: bool,
    pub client_path: Option<String>,
    pub exe_path: Option<String>,
    pub remembered: Option<String>,
    /// Tienda de origen: la UI lo usa para explicar qué hará cada opción.
    pub store: Option<String>,
    pub store_id: Option<String>,
    /// Cuenta de Steam que pide el juego, si no coincide con la activa (guión B.0_1 §3).
    pub account_warning: Option<AvisoCuenta>,
    /// Confirmaciones que exigen sus categorías: `(categoría, pregunta)` — guión F.0_4.
    pub confirmaciones: Vec<(String, String)>,
}

/// El juego requiere una cuenta concreta y ahora mismo hay otra activa.
#[derive(Serialize)]
pub struct AvisoCuenta {
    pub requerida: String,
    pub activa: Option<String>,
}

/// Resuelve si hay que preguntar antes de lanzar, y con qué datos.
pub fn get_launch_options(
    conn: &Connection,
    id: i64,
    executable_id: Option<i64>,
) -> Result<Option<LaunchOptions>> {
    let row = conn
        .query_row(
            "SELECT title, exe_path, client_path, launch_mode, store, store_id, steam_account
             FROM game WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((title, exe_path, client_path, remembered, store, store_id, cuenta)) = row else {
        return Ok(None);
    };
    // Al lanzar un ejecutable adicional concreto el usuario ya ha elegido qué quiere: no se
    // pregunta. El popup es para el botón LANZAR principal.
    // Un juego de Steam siempre tiene "las dos ramas" (abrir Steam o abrir el juego), así
    // que también entra en el popup aunque no tenga `client_path` a mano.
    let tiene_cliente = client_path.as_deref().is_some_and(|p| !p.trim().is_empty())
        || store.as_deref() == Some("steam");
    // Las categorías que exigen confirmación mandan: se pregunta SIEMPRE, aunque haya un
    // modo recordado y aunque se lance un ejecutable adicional (C8).
    let confirmaciones = launch_prompts(conn, id)?;
    let needs_prompt = !confirmaciones.is_empty()
        || (executable_id.is_none() && tiene_cliente && remembered.is_none());

    let account_warning = match (store.as_deref(), cuenta.as_deref()) {
        (Some("steam"), Some(req)) if !req.trim().is_empty() => {
            let activa = crate::steam::cuenta_activa();
            let requerida = req.trim().to_lowercase();
            // Solo avisa si de verdad difiere: si coincide, no molestar.
            (activa.as_deref() != Some(requerida.as_str())).then_some(AvisoCuenta {
                requerida,
                activa,
            })
        }
        _ => None,
    };

    Ok(Some(LaunchOptions {
        title,
        needs_prompt,
        client_path,
        exe_path,
        remembered,
        store,
        store_id,
        account_warning,
        confirmaciones,
    }))
}

/// Recuerda el modo elegido para no volver a preguntar por este juego.
pub fn remember_launch_mode(conn: &Connection, id: i64, mode: LaunchMode) -> Result<()> {
    conn.execute(
        "UPDATE game SET launch_mode = ?2 WHERE id = ?1",
        params![id, mode.as_str()],
    )?;
    Ok(())
}

/// Plan de lanzamiento resuelto (uso interno, ver `game_launch`).
pub struct LaunchPlan {
    /// `None` si el elemento no tiene ejecutable asignado (solo se podrá abrir el cliente).
    pub exe: Option<String>,
    /// Args del ejecutable elegido (del `executable` adicional o `game.exe_args`).
    pub args: Option<String>,
    pub client: Option<String>,
    pub client_args: Option<String>,
    pub client_first: bool,
    /// Id del `executable` adicional lanzado, si no es el principal (para el registro).
    pub executable_id: Option<i64>,
    /// Tienda de origen y su id: si están, el lanzamiento va por el cliente (`steam://`).
    pub store: Option<String>,
    pub store_id: Option<String>,
}

/// Forma canónica de una ruta para compararla con otra (guión A.0_6).
///
/// En Windows `M:\Juegos`, `m:\juegos\` y `M:/Juegos` son **la misma carpeta**, y el usuario las
/// escribe (o las elige en el explorador) de las tres formas. Sin normalizar, cada variante
/// entraba como una fuente distinta y la biblioteca se duplicaba entera.
///
/// *Limitación conocida:* `to_lowercase` sí cubre acentos, pero el índice único de SQLite usa
/// `lower()`, que solo baja ASCII. Dos rutas que se diferencien **solo** en la caja de una letra
/// acentuada podrían colarse; se detectarían aquí, que es por donde entra todo.
pub fn ruta_canonica(path: &str) -> String {
    path.trim()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

/// Añade una fuente, **o devuelve la que ya cubría esa carpeta**.
///
/// Devuelve `(id, ya_existia)`. Cuando ya existía no se inserta nada: se le añade la categoría
/// por defecto que venga (que es lo único nuevo que trae la petición) y se devuelve su id, de
/// modo que el escaneo posterior actualiza esa misma fuente en vez de clonar su biblioteca.
pub fn add_source(
    conn: &Connection,
    kind: &str,
    path: &str,
    default_category_id: Option<i64>,
) -> Result<(i64, bool)> {
    let canonica = ruta_canonica(path);
    let existente: Option<i64> = conn
        .query_row(
            "SELECT id FROM source WHERE kind = ?1 AND LOWER(RTRIM(REPLACE(path, '/', '\\'), '\\')) = ?2
             ORDER BY id LIMIT 1",
            params![kind, canonica],
            |r| r.get(0),
        )
        .optional()?;

    let (id, ya) = match existente {
        Some(id) => (id, true),
        None => {
            conn.execute(
                "INSERT INTO source (kind, path, default_category_id) VALUES (?1, ?2, ?3)",
                params![kind, path, default_category_id],
            )?;
            (conn.last_insert_rowid(), false)
        }
    };
    // La categoría inicial entra también en `source_category`, que es de donde lee el
    // escáner desde F10: si no, se quedaría solo en la columna antigua y no se aplicaría.
    if let Some(cat) = default_category_id {
        conn.execute(
            "INSERT OR IGNORE INTO source_category (source_id, category_id) VALUES (?1, ?2)",
            params![id, cat],
        )?;
    }
    Ok((id, ya))
}

/// Fusiona las fuentes que apuntan a la **misma** carpeta (guión A.0_6 §2).
///
/// Se queda con la de id más bajo —la primera que se añadió— y le pasa todo lo que colgaba de
/// las demás: sus juegos y sus categorías de fuente. Después borra las sobrantes.
///
/// No se borra ningún juego. `game.source_id` solo dice de dónde salió; reapuntarlo a la fuente
/// superviviente es exactamente lo que significa «fusionar».
///
/// Devuelve cuántas fuentes se han eliminado.
pub fn fusionar_fuentes_duplicadas(conn: &Connection) -> Result<usize> {
    let grupos: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, kind, path FROM source ORDER BY id")?;
        let filas = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        filas.collect::<Result<_>>()?
    };

    let mut primera: std::collections::HashMap<(String, String), i64> =
        std::collections::HashMap::new();
    let mut sobrantes: Vec<(i64, i64)> = Vec::new(); // (duplicada, superviviente)
    for (id, kind, path) in grupos {
        let clave = (kind, ruta_canonica(&path));
        match primera.get(&clave) {
            Some(&buena) => sobrantes.push((id, buena)),
            None => {
                primera.insert(clave, id);
            }
        }
    }

    for (dup, buena) in &sobrantes {
        conn.execute(
            "UPDATE game SET source_id = ?2 WHERE source_id = ?1",
            params![dup, buena],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_category (source_id, category_id)
             SELECT ?2, category_id FROM source_category WHERE source_id = ?1",
            params![dup, buena],
        )?;
        conn.execute("DELETE FROM source_category WHERE source_id = ?1", params![dup])?;
        conn.execute("DELETE FROM source WHERE id = ?1", params![dup])?;
    }
    Ok(sobrantes.len())
}

pub fn list_sources(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.kind, s.path, s.default_category_id, s.enabled,
                (SELECT COUNT(*) FROM game g WHERE g.source_id = s.id)
         FROM source s ORDER BY s.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Source {
            id: r.get(0)?,
            kind: r.get(1)?,
            path: r.get(2)?,
            default_category_id: r.get(3)?,
            enabled: r.get::<_, i64>(4)? != 0,
            game_count: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// Activa/desactiva una fuente. Las desactivadas se saltan al escanear, pero sus juegos
/// siguen en la biblioteca.
pub fn set_source_enabled(conn: &Connection, id: i64, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE source SET enabled = ?2 WHERE id = ?1",
        params![id, enabled as i64],
    )?;
    Ok(())
}

/// Elimina una fuente. Con `delete_games`, borra también los juegos que vinieron de ella
/// (y sus categorías, ejecutables y registro); si no, quedan en la biblioteca sin fuente.
/// Devuelve cuántos juegos se han borrado.
pub fn remove_source(conn: &Connection, id: i64, delete_games: bool) -> Result<usize> {
    let mut borrados = 0usize;
    if delete_games {
        conn.execute(
            "DELETE FROM game_category WHERE game_id IN (SELECT id FROM game WHERE source_id = ?1)",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM categoria_rechazada WHERE game_id IN (SELECT id FROM game WHERE source_id = ?1)",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM executable WHERE game_id IN (SELECT id FROM game WHERE source_id = ?1)",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM launch_log WHERE game_id IN (SELECT id FROM game WHERE source_id = ?1)",
            params![id],
        )?;
        borrados = conn.execute("DELETE FROM game WHERE source_id = ?1", params![id])?;
    } else {
        conn.execute(
            "UPDATE game SET source_id = NULL WHERE source_id = ?1",
            params![id],
        )?;
    }
    conn.execute("DELETE FROM source WHERE id = ?1", params![id])?;
    Ok(borrados)
}

/// Deja a cada categoría con el orden alfabético **entre sus hermanas** (A2). Es el punto de
/// partida; a partir de ahí manda el orden manual (A3).
pub fn ordenar_alfabeticamente(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE category SET sort_order = (
             SELECT COUNT(*) FROM category s
             WHERE COALESCE(s.parent_id, -1) = COALESCE(category.parent_id, -1)
               AND s.name COLLATE NOCASE < category.name COLLATE NOCASE
         )",
        [],
    )?;
    Ok(())
}

/// Una categoría y **todas sus descendientes**. Es la base de la «herencia» de ADR-005 §4.1:
/// filtrar por *Nº Jugadores* tiene que traer los de *Couch Co-Op*, *SinglePlayer*, etc.
pub fn descendientes(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE d(id) AS (
             SELECT ?1
             UNION
             SELECT c.id FROM category c JOIN d ON c.parent_id = d.id
         ) SELECT id FROM d",
    )?;
    let filas = stmt.query_map(params![id], |r| r.get::<_, i64>(0))?;
    filas.collect()
}

/// El árbol entero, en orden de recorrido (cada padre seguido de sus hijas).
///
/// Los contadores se calculan aquí y no en SQL a propósito: hay que contar **sin repetir** los
/// juegos de toda la rama, y una sola pasada en Rust sobre unas pocas decenas de filas es más
/// clara que un `WITH RECURSIVE` correlacionado. De paso arregla lo que señalaba ADR-004 §2.2:
/// los contadores **ya no incluyen los excluidos**, así que cuadran con lo que pinta la
/// cuadrícula.
pub fn list_categories(conn: &Connection) -> Result<Vec<Category>> {
    struct Fila {
        id: i64,
        name: String,
        parent_id: Option<i64>,
        sort_order: i64,
        launch_prompt: Option<String>,
    }
    let filas: Vec<Fila> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, sort_order, launch_prompt FROM category",
        )?;
        let it = stmt.query_map([], |r| {
            Ok(Fila {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                sort_order: r.get(3)?,
                launch_prompt: r.get(4)?,
            })
        })?;
        it.collect::<Result<_>>()?
    };

    // Juegos por categoría (solo los visibles).
    let mut propios: HashMap<i64, Vec<i64>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT gc.category_id, gc.game_id
             FROM game_category gc JOIN game g ON g.id = gc.game_id
             WHERE g.state <> 'excluded'",
        )?;
        let it = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for par in it {
            let (cat, juego) = par?;
            propios.entry(cat).or_default().push(juego);
        }
    }

    let mut hijas: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (i, f) in filas.iter().enumerate() {
        hijas.entry(f.parent_id).or_default().push(i);
    }
    for v in hijas.values_mut() {
        v.sort_by(|&a, &b| {
            filas[a]
                .sort_order
                .cmp(&filas[b].sort_order)
                .then_with(|| filas[a].name.to_lowercase().cmp(&filas[b].name.to_lowercase()))
        });
    }

    /// Junta los juegos de la rama sin repetir.
    fn recoger(
        i: usize,
        filas: &[Fila],
        hijas: &HashMap<Option<i64>, Vec<usize>>,
        propios: &HashMap<i64, Vec<i64>>,
        dentro: &mut HashSet<i64>,
    ) {
        if let Some(v) = propios.get(&filas[i].id) {
            dentro.extend(v.iter().copied());
        }
        if let Some(hs) = hijas.get(&Some(filas[i].id)) {
            for &h in hs {
                recoger(h, filas, hijas, propios, dentro);
            }
        }
    }

    // Recorrido en profundidad: cada padre seguido de sus hijas, ya ordenadas.
    let mut salida = Vec::with_capacity(filas.len());
    let mut pila: Vec<usize> = hijas.get(&None).cloned().unwrap_or_default();
    pila.reverse();
    while let Some(i) = pila.pop() {
        let mut dentro = HashSet::new();
        recoger(i, &filas, &hijas, &propios, &mut dentro);
        salida.push(Category {
            id: filas[i].id,
            name: filas[i].name.clone(),
            parent_id: filas[i].parent_id,
            sort_order: filas[i].sort_order,
            count: dentro.len() as i64,
            launch_prompt: filas[i].launch_prompt.clone(),
        });
        if let Some(hs) = hijas.get(&Some(filas[i].id)) {
            for &h in hs.iter().rev() {
                pila.push(h);
            }
        }
    }
    Ok(salida)
}

/// Ejes y categorías que trae la app de fábrica (ADR-005 §6). No se imponen: hay un botón en
/// el editor que **añade las que falten**, y en una instalación nueva se crean solas.
pub const CATEGORIAS_POR_DEFECTO: &[(&str, &[&str])] = &[
    (
        "Clientes",
        &[
            "Steam",
            "Epic Games",
            "GOG",
            "EA",
            "Ubisoft",
            "Battle.net",
            "Xbox / Game Pass",
            "Emulado",
            "Otros",
        ],
    ),
    (
        "Nº Jugadores",
        &[
            "SinglePlayer",
            "Couch Co-Op",
            "Online Co-Op",
            "Split-Screen",
            "Online Multiplayer",
        ],
    ),
    (
        "TAGS",
        &[
            "Acción",
            "Aventura",
            "Estrategia",
            "RPG",
            "Shooter",
            "Survival",
            "Puzzle",
            "Plataformas",
            "Carreras",
            "Deportes",
            "Terror",
            "Simulación",
        ],
    ),
];

/// Nombres que la gente ya usa y que pertenecen a un eje concreto. Sirven **solo para
/// proponer** una recolocación, nunca para aplicarla sola.
const ALIAS_DE_EJE: &[(&str, &str)] = &[
    ("Coop", "Nº Jugadores"),
    ("Co-op", "Nº Jugadores"),
    ("Cooperativo", "Nº Jugadores"),
    ("Coop-Pirate", "Nº Jugadores"),
    ("Pantalla partida", "Nº Jugadores"),
    ("Multijugador", "Nº Jugadores"),
    ("Un jugador", "Nº Jugadores"),
];

/// Añade las categorías por defecto que falten. **Idempotente por nombre**: si ya existe una
/// llamada igual no la duplica ni la mueve de sitio — lo que ya está puesto manda.
/// Devuelve los nombres que se han creado.
pub fn ensure_default_categories(conn: &Connection) -> Result<Vec<String>> {
    let mut creadas = Vec::new();
    for (eje, hijas) in CATEGORIAS_POR_DEFECTO {
        let existia = id_de_categoria(conn, eje)?;
        let eje_id = create_category(conn, eje, None)?;
        if existia.is_none() {
            creadas.push((*eje).to_string());
        }
        for h in *hijas {
            if id_de_categoria(conn, h)?.is_none() {
                create_category(conn, h, Some(eje_id))?;
                creadas.push((*h).to_string());
            }
        }
    }
    Ok(creadas)
}

pub fn id_de_categoria(conn: &Connection, name: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM category WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |r| r.get(0),
    )
    .optional()
}

/// Una categoría que encaja mejor en otro eje. **Solo se propone**: es dato del usuario.
#[derive(Serialize)]
pub struct Recolocacion {
    pub id: i64,
    pub name: String,
    /// Nombre del padre actual; `None` si hoy es un eje raíz.
    pub desde: Option<String>,
    pub hacia: String,
}

/// Categorías cuyo nombre delata que pertenecen a otro eje (ADR-005 §8 #4).
///
/// El caso real que la motiva: *Coop* y *Split-Screen* estaban en *TAGS*, pero dicen **con
/// cuánta gente se juega**, no qué clase de juego es — que es justo la confusión que obligaba
/// a la cuadrícula a repetir juegos.
pub fn sugerir_recolocacion(conn: &Connection) -> Result<Vec<Recolocacion>> {
    let mut esperado: HashMap<String, &str> = HashMap::new();
    for (eje, hijas) in CATEGORIAS_POR_DEFECTO {
        for h in *hijas {
            esperado.insert(h.to_lowercase(), eje);
        }
    }
    for (nombre, eje) in ALIAS_DE_EJE {
        esperado.insert(nombre.to_lowercase(), eje);
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, p.name
         FROM category c LEFT JOIN category p ON p.id = c.parent_id",
    )?;
    let filas = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut out = Vec::new();
    for fila in filas {
        let (id, name, padre) = fila?;
        let Some(eje) = esperado.get(&name.to_lowercase()) else {
            continue;
        };
        // Ya está donde toca, o es el propio eje.
        if padre.as_deref() == Some(*eje) || name.eq_ignore_ascii_case(eje) {
            continue;
        }
        out.push(Recolocacion {
            id,
            name,
            desde: padre,
            hacia: (*eje).to_string(),
        });
    }
    out.sort_by(|a, b| a.hacia.cmp(&b.hacia).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Aplica las recolocaciones que el usuario haya confirmado, creando el eje si falta.
pub fn aplicar_recolocacion(conn: &Connection, ids: &[i64]) -> Result<usize> {
    let sugeridas = sugerir_recolocacion(conn)?;
    let mut n = 0;
    for s in sugeridas.iter().filter(|s| ids.contains(&s.id)) {
        let eje = create_category(conn, &s.hacia, None)?;
        set_category_parent(conn, s.id, Some(eje))?;
        n += 1;
    }
    Ok(n)
}

/// Crea (o reutiliza) una categoría. `parent` = `None` la deja como eje raíz.
pub fn create_category(conn: &Connection, name: &str, parent: Option<i64>) -> Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO category (name, parent_id, sort_order)
         VALUES (?1, ?2, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM category s
                          WHERE COALESCE(s.parent_id, -1) = COALESCE(?2, -1)))",
        params![name, parent],
    )?;
    conn.query_row(
        "SELECT id FROM category WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )
}

// ── F10: categorías de una fuente entera ────────────────────────────────────────────

/// Categorías asignadas a una fuente (se aplican a todo lo que venga de ella).
pub fn source_categories(conn: &Connection, source_id: i64) -> Result<Vec<i64>> {
    let mut stmt =
        conn.prepare("SELECT category_id FROM source_category WHERE source_id = ?1")?;
    let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
    rows.collect()
}

/// Asigna una categoría a una fuente. Con `retroactivo`, se la pone además a **todos** sus
/// juegos, incluidos los del histórico — *"todos los que estén o hayan estado instalados"*.
/// Devuelve cuántos juegos se han etiquetado.
///
/// Respeta el rechazo igual que `assign_category_auto` (guión A.0_8): si el usuario ya le
/// quitó esta categoría a un juego concreto, pulsar «Aplicar a todos» en Fuentes no se lo
/// vuelve a poner. Sin esto, era la misma regla de fuente la que no mandaba al reescanear
/// pero sí mandaba al reafirmarla a mano desde el diálogo de Fuentes.
pub fn assign_source_category(
    conn: &Connection,
    source_id: i64,
    category_id: i64,
    retroactivo: bool,
) -> Result<usize> {
    conn.execute(
        "INSERT OR IGNORE INTO source_category (source_id, category_id) VALUES (?1, ?2)",
        params![source_id, category_id],
    )?;
    if !retroactivo {
        return Ok(0);
    }
    // Los excluidos se quedan fuera (ya dijo que no los quiere ver) y también los que el
    // usuario ha rechazado a mano para esta categoría en concreto.
    conn.execute(
        "INSERT OR IGNORE INTO game_category (game_id, category_id)
         SELECT id, ?2 FROM game
          WHERE source_id = ?1 AND state <> 'excluded'
            AND id NOT IN (SELECT game_id FROM categoria_rechazada WHERE category_id = ?2)",
        params![source_id, category_id],
    )
}

/// Quita una categoría de una fuente. `limpiar` la quita también de sus juegos.
pub fn unassign_source_category(
    conn: &Connection,
    source_id: i64,
    category_id: i64,
    limpiar: bool,
) -> Result<usize> {
    conn.execute(
        "DELETE FROM source_category WHERE source_id = ?1 AND category_id = ?2",
        params![source_id, category_id],
    )?;
    if !limpiar {
        return Ok(0);
    }
    conn.execute(
        "DELETE FROM game_category
         WHERE category_id = ?2 AND game_id IN (SELECT id FROM game WHERE source_id = ?1)",
        params![source_id, category_id],
    )
}

// ── F11/C8: comportamiento de una categoría ─────────────────────────────────────────

/// Mensaje de confirmación previo al lanzamiento (`None` = sin confirmación).
pub fn set_category_prompt(conn: &Connection, id: i64, prompt: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE category SET launch_prompt = ?2 WHERE id = ?1",
        params![id, prompt.filter(|p| !p.trim().is_empty())],
    )?;
    Ok(())
}

/// Confirmaciones que exigen las categorías de un juego, en orden alfabético de categoría.
/// Caso de uso: «Launcher SpaceWar» pregunta siempre si la cuenta de servicio está abierta.
pub fn launch_prompts(conn: &Connection, game_id: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT c.name, c.launch_prompt FROM category c
         JOIN game_category gc ON gc.category_id = c.id
         WHERE gc.game_id = ?1 AND COALESCE(c.launch_prompt, '') <> ''
         ORDER BY c.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![game_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Renombra una categoría. Falla si el nombre ya existe (hay UNIQUE en `category.name`).
pub fn rename_category(conn: &Connection, id: i64, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE category SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    Ok(())
}

/// Cuelga una categoría de otra (`None` = pasa a ser un eje raíz).
///
/// Se niega a crear un ciclo: mover *Clientes* dentro de *Steam*, que es su propia hija,
/// dejaría un trozo del árbol suelto y colgaría cualquier recorrido.
pub fn set_category_parent(conn: &Connection, id: i64, parent: Option<i64>) -> Result<()> {
    if let Some(p) = parent {
        if p == id || descendientes(conn, id)?.contains(&p) {
            return Err(rusqlite::Error::InvalidParameterName(
                "una categoría no puede colgar de sí misma ni de una de sus hijas".into(),
            ));
        }
    }
    conn.execute(
        "UPDATE category SET parent_id = ?2,
                             sort_order = (SELECT COALESCE(MAX(sort_order) + 1, 0)
                                           FROM category s
                                           WHERE COALESCE(s.parent_id, -1) = COALESCE(?2, -1))
         WHERE id = ?1",
        params![id, parent],
    )?;
    Ok(())
}

/// Sube (`-1`) o baja (`+1`) una categoría **entre sus hermanas** (A3: orden manual).
/// Devuelve `false` si ya estaba en el extremo.
pub fn move_category(conn: &Connection, id: i64, delta: i64) -> Result<bool> {
    let padre: Option<i64> = conn.query_row(
        "SELECT parent_id FROM category WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let hermanas: Vec<i64> = {
        let mut st = conn.prepare(
            "SELECT id FROM category WHERE COALESCE(parent_id, -1) = COALESCE(?1, -1)
             ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let it = st.query_map(params![padre], |r| r.get::<_, i64>(0))?;
        it.collect::<Result<_>>()?
    };
    let Some(pos) = hermanas.iter().position(|&h| h == id) else {
        return Ok(false);
    };
    let destino = pos as i64 + delta;
    if destino < 0 || destino as usize >= hermanas.len() {
        return Ok(false);
    }
    let mut orden = hermanas.clone();
    orden.swap(pos, destino as usize);
    // Se reescribe el bloque entero: así el orden queda compacto (0,1,2…) pase lo que pase.
    for (i, cid) in orden.iter().enumerate() {
        conn.execute(
            "UPDATE category SET sort_order = ?2 WHERE id = ?1",
            params![cid, i as i64],
        )?;
    }
    Ok(true)
}

/// Elimina una categoría y sus asignaciones. **No** borra juegos.
/// Sus hijas **no** se borran: suben un nivel, para no perder ramas enteras por un clic.
pub fn delete_category(conn: &Connection, id: i64) -> Result<()> {
    let padre: Option<i64> = conn
        .query_row("SELECT parent_id FROM category WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?
        .flatten();
    conn.execute(
        "UPDATE category SET parent_id = ?2 WHERE parent_id = ?1",
        params![id, padre],
    )?;
    conn.execute("DELETE FROM game_category WHERE category_id = ?1", params![id])?;
    conn.execute("DELETE FROM categoria_rechazada WHERE category_id = ?1", params![id])?;
    conn.execute("DELETE FROM category WHERE id = ?1", params![id])?;
    Ok(())
}

/// Asigna una categoría **porque lo ha pedido el usuario**.
///
/// Levanta el rechazo si lo había: volver a ponerla a mano es decir «esta sí la quiero», y a
/// partir de ahí la regla de la fuente vuelve a tener permiso.
pub fn assign_category(conn: &Connection, game_id: i64, category_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM categoria_rechazada WHERE game_id = ?1 AND category_id = ?2",
        params![game_id, category_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO game_category (game_id, category_id) VALUES (?1, ?2)",
        params![game_id, category_id],
    )?;
    Ok(())
}

/// Asigna una categoría **automáticamente** (la regla de la fuente, al escanear).
///
/// Respeta el rechazo: si el usuario se la quitó a ese juego, **no se le vuelve a poner**
/// (guión A.0_8). Es lo que permite tener una carpeta entera en «Coop» y sacar de ahí el único
/// juego que no lo es, sin que el siguiente escaneo lo deshaga.
pub fn assign_category_auto(conn: &Connection, game_id: i64, category_id: i64) -> Result<bool> {
    let rechazada: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM categoria_rechazada WHERE game_id = ?1 AND category_id = ?2)",
        params![game_id, category_id],
        |r| r.get::<_, i64>(0).map(|n| n != 0),
    )?;
    if rechazada {
        return Ok(false);
    }
    conn.execute(
        "INSERT OR IGNORE INTO game_category (game_id, category_id) VALUES (?1, ?2)",
        params![game_id, category_id],
    )?;
    Ok(true)
}

/// Quita una categoría de un juego **a petición del usuario**, y lo recuerda.
///
/// El recuerdo es la parte importante: sin él, la regla de la fuente se la volvía a poner en el
/// siguiente escaneo —y como escanear pasa al abrir el hub, la decisión del usuario no duraba
/// ni una sesión.
pub fn unassign_category(conn: &Connection, game_id: i64, category_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM game_category WHERE game_id = ?1 AND category_id = ?2",
        params![game_id, category_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO categoria_rechazada (game_id, category_id) VALUES (?1, ?2)",
        params![game_id, category_id],
    )?;
    Ok(())
}

/// Un juego/app encontrado por un proveedor, **antes** de tocar la BD.
/// Vive aquí (y no en `sources`) porque es el contrato de entrada del `upsert`.
#[derive(Debug, Clone, Default)]
pub struct Discovered {
    /// Clave de dedupe: carpeta del juego, ruta del exe, o `steam:<appid>`.
    pub source_key: String,
    pub title: String,
    pub install_dir: Option<String>,
    pub exe_path: Option<String>,
    pub cover_path: Option<String>,
    pub platform: String,
    /// Tienda de origen (`steam`…) y su identificador propio (appid).
    pub store: Option<String>,
    pub store_id: Option<String>,
}

/// Qué ha pasado con un elemento al escanear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    Created,
    Updated,
    /// Estaba **excluido**: el escáner lo deja intacto (guión L.0_1 §2).
    Skipped,
}

/// Inserta o actualiza un juego dedupando por `source_key`.
/// En update **no** pisa `exe_path` si el usuario lo fijó a mano (`exe_locked`).
pub fn upsert_game(conn: &Connection, source_id: i64, d: &Discovered) -> Result<(i64, Upsert)> {
    let Discovered {
        source_key,
        title,
        install_dir,
        exe_path,
        cover_path,
        platform,
        store,
        store_id,
    } = d;
    let platform = if platform.is_empty() { "PC" } else { platform };
    // La misma carpeta escrita con otra caja o con `/` **es la misma carpeta** (guión A.0_6
    // §1.1). Comparar las cadenas tal cual hacía que un cambio de grafía no reconociera el
    // juego y lo insertara otra vez, perdiendo su estado, su nombre y sus categorías.
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, state FROM game
              WHERE LOWER(RTRIM(REPLACE(source_key, '/', '\\'), '\\')) = ?1
              ORDER BY id LIMIT 1",
            params![ruta_canonica(source_key)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, state)) = existing {
        // Regla crítica de L.0_1: un elemento **excluido nunca se resucita**. El dedupe por
        // `source_key` reconoce la carpeta y el escáner la deja tal cual.
        if state == STATE_EXCLUDED {
            return Ok((id, Upsert::Skipped));
        }
        // En re-escaneo actualizamos título/carátula, y corregimos exe_path SALVO que el
        // usuario lo haya fijado a mano (exe_locked = 1). Ver guión C.
        // La carátula sigue la misma regla desde D.0_4: con `cover_locked = 1` no se toca.
        // Antes, encontrar una imagen en la carpeta **pisaba la que el usuario había elegido**.
        // Pasa a `installed`: se ha (vuelto a) encontrar en disco — también si era un
        // "deseado" que el usuario acaba de instalar.
        // Hay ejecutable si el escáner ha encontrado uno, o si el juego se lanza por su tienda.
        let hay_exe = exe_path.as_deref().is_some_and(|e| !e.trim().is_empty())
            || store_id.as_deref().is_some_and(|s| !s.trim().is_empty());
        conn.execute(
            // Lo que el usuario ha curado **no se pisa** (guión A.0_7):
            //  · el título, si lo renombró (`title_locked`);
            //  · el ejecutable, si lo fijó (`exe_locked`);
            //  · la carátula, si eligió una (`cover_locked`).
            //
            // Y el estado deja de ponerse a `installed` a ciegas. Encontrar una carpeta **no
            // es** encontrar un juego: de Tom Clancy's Ghost Recon Wildlands queda el
            // directorio en Uplay —con capturas y poco más— y cada escaneo lo resucitaba como
            // «instalado, sin ejecutable», que es justo lo contrario de lo que decía el usuario.
            "UPDATE game SET
                 title = CASE WHEN title_locked = 1 THEN title ELSE ?2 END,
                 install_dir = ?3, source_id = ?4,
                 cover_path = CASE WHEN cover_locked = 1 THEN cover_path
                                   ELSE COALESCE(?5, cover_path) END,
                 exe_path = CASE WHEN exe_locked = 1 THEN exe_path ELSE ?6 END,
                 store = COALESCE(?7, store), store_id = COALESCE(?8, store_id),
                 state = CASE WHEN state = 'uninstalled' AND ?9 = 0
                              THEN 'uninstalled' ELSE 'installed' END
             WHERE id = ?1",
            params![
                id, title, install_dir, source_id, cover_path, exe_path, store, store_id,
                hay_exe as i64
            ],
        )?;
        Ok((id, Upsert::Updated))
    } else {
        conn.execute(
            "INSERT INTO game (source_id, source_key, title, install_dir, exe_path, platform,
                               cover_path, store, store_id, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
            params![
                source_id, source_key, title, install_dir, exe_path, platform, cover_path,
                store, store_id
            ],
        )?;
        Ok((conn.last_insert_rowid(), Upsert::Created))
    }
}

/// Marca o desmarca un juego como favorito. Es independiente del estado: un juego del
/// histórico puede seguir siendo favorito.
pub fn set_favorite(conn: &Connection, id: i64, favorite: bool) -> Result<()> {
    conn.execute(
        "UPDATE game SET favorite = ?2 WHERE id = ?1",
        params![id, favorite as i64],
    )?;
    Ok(())
}

/// Carpeta y ejecutable registrados de un juego, para abrirlo en el explorador (A.0_5 §2).
///
/// Las dos pueden faltar, y no son redundantes: una app añadida a mano suele tener solo
/// `exe_path`, y un juego de tienda sin ejecutable resuelto solo `install_dir`. Quien llama
/// decide con cuál se queda; aquí no se mira el disco.
pub fn rutas_de(conn: &Connection, id: i64) -> Result<(Option<String>, Option<String>)> {
    conn.query_row(
        "SELECT NULLIF(install_dir, ''), NULLIF(exe_path, '') FROM game WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

/// Cambia el estado de un elemento (excluir, restaurar, marcar como no instalado…).
pub fn set_state(conn: &Connection, id: i64, state: &str) -> Result<()> {
    conn.execute(
        "UPDATE game SET state = ?2 WHERE id = ?1",
        params![id, state],
    )?;
    Ok(())
}

/// Contadores por estado, para la sidebar.
/// Contadores de la barra lateral. `dias_recientes` fija la ventana de «añadidos hace poco».
pub fn state_counts(conn: &Connection, dias_recientes: i64) -> Result<StateCounts> {
    let mut c = StateCounts::default();
    let mut stmt = conn.prepare("SELECT state, COUNT(*) FROM game GROUP BY state")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (state, n) = row?;
        match state.as_str() {
            STATE_INSTALLED => c.installed = n,
            STATE_UNINSTALLED => c.uninstalled = n,
            STATE_EXCLUDED => c.excluded = n,
            _ => {}
        }
    }
    c.favorites = conn.query_row(
        "SELECT COUNT(*) FROM game WHERE favorite = 1 AND state <> 'excluded'",
        [],
        |r| r.get(0),
    )?;
    // Añadidos hace poco. La ventana la fija la UI y se pasa por parámetro.
    // Nota: la lleva `state_counts_con` — aquí queda el valor por defecto.
    // Los que no se pueden lanzar: ni exe ni id de tienda. Es una vista de mantenimiento,
    // así que solo se enseña cuando hay alguno (guión F.0_3 §3).
    c.needs_exe = conn.query_row(
        "SELECT COUNT(*) FROM game
         WHERE COALESCE(exe_path, '') = '' AND COALESCE(store_id, '') = ''
           AND state <> 'excluded'",
        [],
        |r| r.get(0),
    )?;
    c.recent = conn.query_row(
        "SELECT COUNT(*) FROM game
         WHERE added_at IS NOT NULL AND added_at >= datetime('now', ?1)
           AND state <> 'excluded'",
        params![format!("-{} days", dias_recientes.max(1))],
        |r| r.get(0),
    )?;
    Ok(c)
}

/// Orígenes presentes en la biblioteca (`kind` de las fuentes con juegos).
pub fn list_origins(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.kind FROM source s
         WHERE EXISTS (SELECT 1 FROM game g WHERE g.source_id = s.id)
         ORDER BY s.kind",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Plataformas distintas presentes en la biblioteca (para el desplegable de filtros).
pub fn list_platforms(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT platform FROM game WHERE platform <> '' ORDER BY platform COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Marca como `uninstalled` los juegos de una fuente cuyo `source_key` ya no aparece en disco.
/// **No borra**: conserva categorías, ediciones manuales, ejecutables y tiempo jugado. No toca
/// los excluidos ni los deseados. Devuelve cuántos han quedado marcados.
pub fn mark_uninstalled(conn: &Connection, source_id: i64, found_keys: &[String]) -> Result<usize> {
    let placeholders = if found_keys.is_empty() {
        "''".to_string()
    } else {
        vec!["?"; found_keys.len()].join(",")
    };
    // `source_id` es un id interno (no entrada de usuario) → interpolarlo es seguro y evita
    // mezclar tipos con los placeholders de las claves.
    let sql = format!(
        "UPDATE game SET state = 'uninstalled'
         WHERE source_id = {} AND state = 'installed' AND source_key NOT IN ({})",
        source_id, placeholders
    );
    conn.execute(&sql, rusqlite::params_from_iter(found_keys.iter()))?;
    conn.query_row(
        "SELECT COUNT(*) FROM game WHERE source_id = ?1 AND state = 'uninstalled'",
        params![source_id],
        |r| r.get(0),
    )
    .map(|n: i64| n as usize)
}

// ── Ajustes del core (los de la UI viven en localStorage; ver guión H.0_1) ───────────

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM setting WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Lo que la sincronización trae de TimeTrack para un ejecutable.
///
/// Era una tupla `(i64, Option<String>)` y al entrar el tercer campo dejó de leerse: `.0` y `.1`
/// no dicen cuál es el tiempo y cuál el arte.
pub struct DatosRemotos {
    pub total_secs: i64,
    /// Arte a usar si el juego no tiene ninguna. `None` = no tocar nada.
    pub arte: Option<String>,
    /// Id en TimeTrack, para poder abrirle sus gráficas después (guión T.0_2).
    pub app_id: i64,
}

/// Vuelca en la caché local el tiempo jugado y el arte leídos de TimeTrack.
/// Correlaciona por `exe_name` (ver `ARCHITECTURE/02-integracion-timetrack.md` §2).
/// Devuelve `(juegos con tiempo, juegos con arte nueva)`.
pub fn sync_timetrack(
    conn: &Connection,
    remotas: &std::collections::HashMap<String, DatosRemotos>,
) -> Result<(usize, usize)> {
    let juegos: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, exe_path FROM game WHERE COALESCE(exe_path, '') <> ''",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<Vec<_>>>()?
    };

    let (mut con_tiempo, mut con_arte) = (0usize, 0usize);
    for (id, exe_path) in juegos {
        let clave = crate::timetrack::exe_name(&exe_path);
        let Some(d) = remotas.get(&clave) else {
            continue;
        };
        conn.execute(
            "INSERT INTO playtime_cache (game_id, exe_name, total_secs, last_sync, tt_app_id)
             VALUES (?1, ?2, ?3, datetime('now'), ?4)
             ON CONFLICT(game_id) DO UPDATE SET
                 exe_name = excluded.exe_name,
                 total_secs = excluded.total_secs,
                 last_sync = excluded.last_sync,
                 tt_app_id = excluded.tt_app_id",
            params![id, clave, d.total_secs, d.app_id],
        )?;
        if d.total_secs > 0 {
            con_tiempo += 1;
        }
        if let Some(a) = &d.arte {
            // Solo rellena si no había arte: nunca pisa una carátula local del usuario.
            let n = conn.execute(
                "UPDATE game SET art_path = ?2 WHERE id = ?1 AND COALESCE(art_path, '') = ''",
                params![id, a],
            )?;
            con_arte += n;
        }
    }
    Ok((con_tiempo, con_arte))
}

/// Un juego al que le falta carátula, con lo necesario para buscarle una (guión D.0_1).
pub struct SinArte {
    pub id: i64,
    pub title: String,
    pub exe_path: Option<String>,
    pub store_id: Option<String>,
    pub install_dir: Option<String>,
}

/// Juegos sin carátula: ni la local del usuario (`cover_path`) ni arte ya resuelta
/// (`art_path`). Los excluidos no se tocan.
pub fn list_sin_arte(conn: &Connection) -> Result<Vec<SinArte>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, exe_path, store_id, install_dir FROM game
         WHERE COALESCE(cover_path, '') = '' AND COALESCE(art_path, '') = ''
           AND state <> 'excluded'
         ORDER BY title COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SinArte {
            id: r.get(0)?,
            title: r.get(1)?,
            exe_path: r.get(2)?,
            store_id: r.get(3)?,
            install_dir: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Guarda la tienda y su id deducidos (p. ej. el appid sacado de `steamapps\common`).
pub fn set_store(conn: &Connection, id: i64, store: &str, store_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE game SET store = ?2, store_id = ?3 WHERE id = ?1",
        params![id, store, store_id],
    )?;
    Ok(())
}

/// Los mismos datos que `list_sin_arte` pero de **un** juego, tenga arte o no.
///
/// Lo usa «🔍 Buscar automáticamente» desde el editor: ahí no se filtra por si falta carátula
/// —el usuario está pidiendo explícitamente que se vuelva a mirar—, pero la cascada necesita
/// recibir exactamente lo mismo que en el lote para poder ser **una sola** (guión D.0_4 §2).
pub fn sin_arte_uno(conn: &Connection, id: i64) -> Result<Option<SinArte>> {
    conn.query_row(
        "SELECT id, title, exe_path, store_id, install_dir FROM game WHERE id = ?1",
        params![id],
        |r| {
            Ok(SinArte {
                id: r.get(0)?,
                title: r.get(1)?,
                exe_path: r.get(2)?,
                store_id: r.get(3)?,
                install_dir: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Fija (o quita) la carátula **elegida a mano**.
///
/// Marca `cover_locked`, igual que editar el ejecutable marca `exe_locked`: desde ese momento
/// ni el re-escaneo ni «Rehacer» la tocan. Al quitarla se libera el seguro y manda otra vez la
/// cascada, que es justo lo que uno espera de «Quitar la mía».
pub fn set_cover(conn: &Connection, id: i64, path: Option<&str>) -> Result<()> {
    let limpia = path.map(str::trim).filter(|p| !p.is_empty());
    conn.execute(
        "UPDATE game SET cover_path = ?2, cover_locked = ?3 WHERE id = ?1",
        params![id, limpia, limpia.is_some() as i64],
    )?;
    Ok(())
}

/// Deshace las carátulas que el editor **ascendió a manuales sin que nadie lo pidiera**
/// (guión D.0_4 §1).
///
/// El fallo: `get_game` devolvía `COALESCE(cover_path, art_path)` en el campo `cover_path`, el
/// editor lo cargaba en su caja de «carátula propia» y al pulsar *Guardar* lo escribía en la
/// columna de verdad. Bastaba **abrir un juego en Editar y guardar** para que su icono del
/// `.exe` quedara marcado como elección del usuario — y a partir de ahí intocable: ni «Rehacer»
/// ni la cascada vuelven a mirar un juego que ya tiene carátula propia.
///
/// Se reconocen por la ruta: apuntan a **nuestra** carpeta `icons`, donde solo escribimos
/// nosotros. Una elegida de verdad apunta a donde el usuario fue a buscarla. Vuelven a
/// `art_path`, que es lo que siempre fueron; **no se borra ninguna imagen**.
fn reparar_caratulas_ascendidas(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE game
            SET art_path = COALESCE(art_path, cover_path),
                cover_path = NULL
          WHERE cover_locked = 0
            AND cover_path IS NOT NULL
            AND (cover_path LIKE '%\\icons\\%' OR cover_path LIKE '%/icons/%')",
        [],
    )
}

/// Olvida la carátula automática de **un** juego, para poder volver a buscarla.
pub fn clear_art_one(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("UPDATE game SET art_path = NULL WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn set_art(conn: &Connection, id: i64, path: &str) -> Result<()> {
    conn.execute("UPDATE game SET art_path = ?2 WHERE id = ?1", params![id, path])?;
    Ok(())
}

/// Olvida **todo lo automático** para volver a buscarlo desde cero: el arte resuelto por la
/// cascada y, si no está bajo seguro, también la imagen que encontró el escaneo en la carpeta.
///
/// Lo segundo es lo que hace que «Rehacer» sirva de algo. Antes solo se limpiaba `art_path`, y
/// como `list_sin_arte` salta cualquier juego que tenga `cover_path`, los que llevaban una
/// imagen automática en esa columna **no se volvían a mirar nunca**.
///
/// `cover_locked = 1` —la que eligió el usuario— no se toca jamás.
pub fn clear_art(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE game SET art_path = NULL,
                         cover_path = CASE WHEN cover_locked = 1 THEN cover_path ELSE NULL END",
        [],
    )
}

/// Registra un lanzamiento (guión A §9 criterio 6; base de la correlación con TimeTrack, M2).
pub fn log_launch(
    conn: &Connection,
    game_id: i64,
    executable_id: Option<i64>,
    exe_path: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO launch_log (game_id, executable_id, exe_path) VALUES (?1, ?2, ?3)",
        params![game_id, executable_id, exe_path],
    )?;
    Ok(())
}

pub fn list_games(conn: &Connection, f: &LibraryFilter) -> Result<Vec<GameCard>> {
    use rusqlite::types::Value;

    // El grid se pinta desde `state` (calculado en el escaneo): ni un `stat` por juego.
    let map = |r: &rusqlite::Row| {
        // Un juego de tienda no necesita `exe_path`: se lanza por su cliente (`steam://`).
        let store_id: Option<String> = r.get(9)?;
        let exe: Option<String> = if store_id.as_deref().unwrap_or("").is_empty() {
            r.get(4)?
        } else {
            Some(String::from("store"))
        };
        // `group_concat` devuelve "3,7,12" (o NULL si no tiene ninguna).
        let cats: Option<String> = r.get(11)?;
        let categories = cats
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        Ok(GameCard {
            id: r.get(0)?,
            title: r.get(1)?,
            platform: r.get(2)?,
            cover_path: r.get(3)?,
            state: r.get(5)?,
            needs_exe: exe.as_deref().unwrap_or("").is_empty(),
            players_min: r.get(6)?,
            players_max: r.get(7)?,
            playtime_secs: r.get(8)?,
            store_id,
            favorite: r.get::<_, i64>(10)? != 0,
            categories,
            origin: r.get(12)?,
            has_dir: r.get::<_, i64>(13)? != 0,
        })
    };

    // La carátula local manda; si no hay, se usa el arte traído de TimeTrack (D1).
    // El `LEFT JOIN source` ya no es condicional: el origen viaja siempre en la card porque
    // la cuadrícula puede seccionarse por él.
    let mut sql = String::from(
        "SELECT g.id, g.title, g.platform, COALESCE(g.cover_path, g.art_path),
                g.exe_path, g.state, g.players_min, g.players_max,
                COALESCE(pc.total_secs, 0), g.store_id, g.favorite,
                (SELECT group_concat(m.category_id) FROM game_category m WHERE m.game_id = g.id),
                s.kind,
                COALESCE(NULLIF(g.install_dir, ''), NULLIF(g.exe_path, '')) IS NOT NULL
         FROM game g
         LEFT JOIN playtime_cache pc ON pc.game_id = g.id
         LEFT JOIN source s ON s.id = g.source_id",
    );
    let mut cond: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();

    if let Some(cat) = f.category_id {
        // Herencia (ADR-005 §4.1): pedir *Nº Jugadores* trae también los de *Couch Co-Op*.
        // Si la categoría es una hoja, la lista es ella sola y el `IN` se comporta igual.
        let rama = descendientes(conn, cat)?;
        sql.push_str(" JOIN game_category gc ON gc.game_id = g.id");
        cond.push(format!(
            "gc.category_id IN ({})",
            rama.iter().map(|_| "?").collect::<Vec<_>>().join(",")
        ));
        args.extend(rama.into_iter().map(Value::Integer));
    }
    if let Some(o) = f.origin.as_deref().filter(|o| !o.is_empty()) {
        cond.push("COALESCE(s.kind, '') = ?".into());
        args.push(Value::Text(o.to_string()));
    }
    if f.favorite == Some(true) {
        cond.push("g.favorite = 1".into());
    }
    if let Some(dias) = f.added_days.filter(|d| *d > 0) {
        cond.push("g.added_at IS NOT NULL AND g.added_at >= datetime('now', ?)".into());
        args.push(Value::Text(format!("-{} days", dias)));
    }
    if let Some(jugado) = f.played {
        cond.push(
            if jugado {
                "COALESCE(pc.total_secs, 0) > 0"
            } else {
                "COALESCE(pc.total_secs, 0) = 0"
            }
            .into(),
        );
    }
    // Las horas se comparan en segundos, que es como están guardadas: convertir la columna a
    // horas en el SQL impediría usar el índice y, sobre todo, mete redondeos donde no hacen falta.
    if let Some(h) = f.min_hours.filter(|h| *h > 0.0) {
        cond.push("COALESCE(pc.total_secs, 0) >= ?".into());
        args.push(Value::Integer((h * 3600.0).round() as i64));
    }
    if let Some(h) = f.max_hours.filter(|h| *h > 0.0) {
        cond.push("COALESCE(pc.total_secs, 0) < ?".into());
        args.push(Value::Integer((h * 3600.0).round() as i64));
    }
    match f.state.as_deref() {
        Some(s) => {
            cond.push("g.state = ?".into());
            args.push(Value::Text(s.to_string()));
        }
        // Por defecto los excluidos no aparecen: hay que pedirlos explícitamente.
        None => cond.push(format!("g.state <> '{}'", STATE_EXCLUDED)),
    }
    if let Some(q) = f.query.as_deref().filter(|q| !q.trim().is_empty()) {
        cond.push("g.title LIKE ?".into());
        args.push(Value::Text(format!("%{}%", q)));
    }
    if let Some(p) = f.platform.as_deref().filter(|p| !p.is_empty()) {
        cond.push("g.platform = ?".into());
        args.push(Value::Text(p.to_string()));
    }
    if let Some(n) = f.players {
        // "admite al menos N jugadores"; los que no tienen el dato quedan fuera.
        cond.push("COALESCE(g.players_max, 0) >= ?".into());
        args.push(Value::Integer(n));
    }
    if let Some(needs) = f.needs_exe {
        // "Sin ejecutable" incluye no tener id de tienda: un juego de Steam se lanza por
        // `steam://` y no necesita exe, así que no debe salir aquí.
        cond.push(
            if needs {
                "COALESCE(g.exe_path, '') = '' AND COALESCE(g.store_id, '') = ''"
            } else {
                "(COALESCE(g.exe_path, '') <> '' OR COALESCE(g.store_id, '') <> '')"
            }
            .into(),
        );
    }
    if !cond.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&cond.join(" AND "));
    }
    let criterio = f.sort.as_deref().unwrap_or("title");
    // Cada criterio tiene una dirección **natural**: en «tiempo jugado» lo que se quiere ver
    // primero es lo más jugado, no lo menos. Solo el título sube de la A a la Z.
    let desc = f.desc.unwrap_or(criterio != "title");
    let expr = match criterio {
        "playtime" => "COALESCE(pc.total_secs, 0)",
        "recent" => "(SELECT MAX(l.launched_at) FROM launch_log l WHERE l.game_id = g.id)",
        "players" => "COALESCE(g.players_max, 0)",
        // Con fecha real se usa; para lo anterior a que existiera la columna, el id sigue
        // siendo el sucedáneo honesto (entraron en ese orden).
        "added" => "COALESCE(g.added_at, '0000') || printf('%012d', g.id)",
        _ => "g.title COLLATE NOCASE",
    };
    // `(expr) IS NULL` primero manda los «sin dato» al final **en las dos direcciones**: un
    // juego que nunca se ha lanzado no es «el más antiguo», es que no hay dato.
    // El título siempre es el criterio de desempate, para que el orden sea estable.
    sql.push_str(&format!(
        " ORDER BY ({e}) IS NULL, ({e}) {d}, g.title COLLATE NOCASE",
        e = expr,
        d = if desc { "DESC" } else { "ASC" },
    ));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), map)?;
    rows.collect()
}

pub fn list_executables(conn: &Connection, game_id: i64) -> Result<Vec<Executable>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, path, args FROM executable WHERE game_id = ?1 ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![game_id], |r| {
        Ok(Executable {
            id: r.get(0)?,
            label: r.get(1)?,
            path: r.get(2)?,
            args: r.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn add_executable(
    conn: &Connection,
    game_id: i64,
    label: &str,
    path: &str,
    args: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO executable (game_id, label, path, args) VALUES (?1, ?2, ?3, ?4)",
        params![game_id, label, path, args],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_executable(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM executable WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_game(conn: &Connection, id: i64) -> Result<Option<GameDetail>> {
    let mut game = match conn
        .query_row(
            // Las dos por separado, **sin `COALESCE`**: quien edita tiene que poder distinguir
            // «la que puse yo» de «la que encontró la cascada», y sobre todo guardar sin que la
            // segunda se convierta en la primera.
            "SELECT id, title, install_dir, exe_path, platform, cover_path,
                    client_path, client_args, client_first, exe_args, state,
                    launch_mode, players_min, players_max, store, store_id, steam_account,
                    favorite, art_path
             FROM game WHERE id = ?1",
            params![id],
            |r| {
                Ok(GameDetail {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    install_dir: r.get(2)?,
                    exe_path: r.get(3)?,
                    platform: r.get(4)?,
                    cover_path: r.get(5)?,
                    client_path: r.get(6)?,
                    client_args: r.get(7)?,
                    client_first: r.get::<_, i64>(8)? != 0,
                    exe_args: r.get(9)?,
                    state: r.get(10)?,
                    launch_mode: r.get(11)?,
                    players_min: r.get(12)?,
                    players_max: r.get(13)?,
                    store: r.get(14)?,
                    store_id: r.get(15)?,
                    steam_account: r.get(16)?,
                    favorite: r.get::<_, i64>(17)? != 0,
                    art_path: r.get(18)?,
                    exe_exists: false,
                    launch_count: 0,
                    last_launched: None,
                    playtime_secs: 0,
                    playtime_sync: None,
                    tt_app_id: None,
                    categories: Vec::new(),
                    executables: Vec::new(),
                })
            },
        )
        .optional()?
    {
        Some(g) => g,
        None => return Ok(None),
    };
    // Comprobación en vivo (1 solo stat: estamos en la card de detalle, no en el grid).
    game.exe_exists = game
        .exe_path
        .as_deref()
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false);
    let (count, last): (i64, Option<String>) = conn.query_row(
        "SELECT COUNT(*), MAX(launched_at) FROM launch_log WHERE game_id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    game.launch_count = count;
    game.last_launched = last;
    if let Some((secs, sync, tt)) = conn
        .query_row(
            "SELECT total_secs, last_sync, tt_app_id FROM playtime_cache WHERE game_id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()?
    {
        game.playtime_secs = secs;
        game.playtime_sync = sync;
        game.tt_app_id = tt;
    }
    let mut stmt = conn.prepare(
        "SELECT c.name FROM category c JOIN game_category gc ON gc.category_id = c.id
         WHERE gc.game_id = ?1 ORDER BY c.name",
    )?;
    for c in stmt.query_map(params![id], |r| r.get::<_, String>(0))? {
        game.categories.push(c?);
    }
    game.executables = list_executables(conn, id)?;
    Ok(Some(game))
}

/// Actualiza los datos editables de un juego (guión C §3).
#[allow(clippy::too_many_arguments)]
/// Datos editables de un juego. En struct y no como parámetros sueltos: son doce campos y
/// crecen con cada apartado (args, jugadores, modo de lanzamiento, cuenta de Steam…).
#[derive(Default)]
pub struct GameEdit<'a> {
    pub title: &'a str,
    pub platform: &'a str,
    pub exe_path: Option<&'a str>,
    pub exe_args: Option<&'a str>,
    pub client_path: Option<&'a str>,
    pub client_args: Option<&'a str>,
    pub client_first: bool,
    pub players_min: Option<i64>,
    pub players_max: Option<i64>,
    /// `None` = volver a preguntar al lanzar; `Some("game"|"client")` = modo recordado.
    pub launch_mode: Option<&'a str>,
    /// Cuenta de Steam que requiere el juego (guión B.0_1 §3).
    pub steam_account: Option<&'a str>,
}

pub fn update_game(conn: &Connection, id: i64, e: &GameEdit) -> Result<()> {
    let GameEdit {
        title,
        platform,
        exe_path,
        exe_args,
        client_path,
        client_args,
        client_first,
        players_min,
        players_max,
        launch_mode,
        steam_account,
    } = *e;
    let platform = if platform.is_empty() { "PC" } else { platform };
    // Marca exe_locked = 1: a partir de ahora el re-escaneo no pisará el exe_path curado.
    //
    // El título se bloquea **solo si de verdad ha cambiado** (guión A.0_7 §2). Bloquearlo en
    // cada guardado sería repetir el fallo de `D.0_4`: abrir un juego en Editar y guardar sin
    // tocar nada lo congelaría, y el escaneo dejaría de corregir el nombre de la carpeta.
    // En SQLite la parte derecha de un `SET` ve los valores **anteriores** de la fila, así que
    // `title <> ?2` compara el nombre guardado con el nuevo.
    conn.execute(
        "UPDATE game SET title = ?2, platform = ?3, exe_path = ?4,
             client_path = ?5, client_args = ?6, client_first = ?7, exe_args = ?8,
             players_min = ?9, players_max = ?10, launch_mode = ?11, steam_account = ?12,
             exe_locked = 1,
             title_locked = CASE WHEN title <> ?2 THEN 1 ELSE title_locked END
         WHERE id = ?1",
        params![
            id,
            title,
            platform,
            exe_path,
            client_path,
            client_args,
            client_first as i64,
            exe_args,
            players_min,
            players_max,
            launch_mode,
            steam_account
        ],
    )?;
    Ok(())
}

pub fn delete_game(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM game_category WHERE game_id = ?1", params![id])?;
    conn.execute("DELETE FROM categoria_rechazada WHERE game_id = ?1", params![id])?;
    conn.execute("DELETE FROM executable WHERE game_id = ?1", params![id])?;
    conn.execute("DELETE FROM launch_log WHERE game_id = ?1", params![id])?;
    conn.execute("DELETE FROM playtime_cache WHERE game_id = ?1", params![id])?;
    conn.execute("DELETE FROM game WHERE id = ?1", params![id])?;
    Ok(())
}

/// Resuelve el plan de lanzamiento: ejecutable elegido (o el principal) + cliente previo.
pub fn get_launch(
    conn: &Connection,
    id: i64,
    executable_id: Option<i64>,
) -> Result<Option<LaunchPlan>> {
    let game = conn
        .query_row(
            "SELECT exe_path, exe_args, client_path, client_args, client_first, store, store_id
             FROM game WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)? != 0,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((exe_path, exe_args, client, client_args, client_first, store, store_id)) = game
    else {
        return Ok(None);
    };
    // Cada ejecutable lleva SUS args: los del adicional elegido, o los del principal.
    // (C.0_2 §1: antes se leía solo `path` y los args se descartaban en silencio.)
    let (exe, args) = if let Some(eid) = executable_id {
        match conn
            .query_row(
                "SELECT path, args FROM executable WHERE id = ?1 AND game_id = ?2",
                params![eid, id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?
        {
            Some((p, a)) => (Some(p), a),
            None => (None, None),
        }
    } else {
        (exe_path, exe_args)
    };
    // Al lanzar un ejecutable adicional concreto se respeta esa elección: no se desvía por
    // la tienda. El `steam://` es para el botón LANZAR principal.
    let (store, store_id) = if executable_id.is_some() {
        (None, None)
    } else {
        (store, store_id)
    };
    Ok(Some(LaunchPlan {
        exe,
        args,
        client,
        client_args,
        client_first,
        executable_id,
        store,
        store_id,
    }))
}
