// SofaCervecero — core Tauri (M1). Arquitectura: DOCUMENTATION/ARCHITECTURE.
mod art;
mod arte_web;
mod cursor;
mod launcher;
mod library;
#[cfg(windows)]
mod mando;
mod metadata;
#[cfg(windows)]
mod pantallas;
mod service;
mod sources;
mod steam;
mod timetrack;
mod web;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Estado compartido: conexión SQLite protegida por Mutex.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    /// Tareas que el usuario ha pedido cancelar, por id (guión J.0_1).
    canceladas: Mutex<std::collections::HashSet<String>>,
}

impl AppState {
    /// ¿Se ha pedido cancelar esta tarea? Consulta barata: se llama en cada elemento.
    fn cancelada(&self, id: &str) -> bool {
        self.canceladas
            .lock()
            .map(|c| c.contains(id))
            .unwrap_or(false)
    }
    fn olvidar_cancelacion(&self, id: &str) {
        if let Ok(mut c) = self.canceladas.lock() {
            c.remove(id);
        }
    }
}

/// Marca una tarea en curso para que se pare en el siguiente elemento.
/// No mata nada a la fuerza: la tarea termina ordenadamente y deja lo ya hecho.
#[tauri::command]
fn task_cancel(state: State<AppState>, task_id: String) -> Result<(), String> {
    state
        .canceladas
        .lock()
        .map_err(|e| e.to_string())?
        .insert(task_id);
    Ok(())
}

/// Progreso del escaneo (evento `scan_progress`, guión A §8).
#[derive(Clone, serde::Serialize)]
struct ScanProgress {
    source_index: usize,
    source_total: usize,
    source_path: String,
    current: String,
    found: usize,
}

/// Resumen del escaneo (evento `scan_finished` + valor de retorno de `scan_run`).
#[derive(Clone, Default, serde::Serialize)]
struct ScanSummary {
    scanned: usize,
    added: usize,
    updated: usize,
    /// Encontrados pero **excluidos** por el usuario: intactos.
    excluded: usize,
    /// Ya no están en disco → pasan al histórico (`uninstalled`).
    uninstalled: usize,
    /// Fuentes cuya ruta no era accesible (disco desconectado): se dejan intactas.
    skipped: Vec<String>,
    /// El usuario lo paró a medias; lo escaneado hasta ese punto se conserva.
    cancelada: bool,
}

/// Info básica de la app para la UI (nombre/versión/codename).
#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "name": "SofaCervecero",
        "codename": "SofaCervecero",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
fn sources_add(
    state: State<AppState>,
    kind: String,
    path: String,
    default_category_id: Option<i64>,
) -> Result<FuenteAnadida, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (id, ya_existia) = library::add_source(&conn, &kind, &path, default_category_id)
        .map_err(|e| e.to_string())?;
    Ok(FuenteAnadida { id, ya_existia })
}

/// Resultado de añadir una fuente. `ya_existia` deja que la UI avise en vez de callarse: antes
/// se podía añadir la misma carpeta diez veces y la biblioteca salía diez veces (guión A.0_6).
#[derive(serde::Serialize)]
struct FuenteAnadida {
    id: i64,
    ya_existia: bool,
}

#[tauri::command]
fn sources_list(state: State<AppState>) -> Result<Vec<library::Source>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list_sources(&conn).map_err(|e| e.to_string())
}

/// Elimina una fuente. `delete_games`: borrar también los juegos que vinieron de ella.
#[tauri::command]
fn sources_remove(state: State<AppState>, id: i64, delete_games: bool) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::remove_source(&conn, id, delete_games).map_err(|e| e.to_string())
}

/// Activa/desactiva una fuente (las desactivadas se saltan al escanear).
#[tauri::command]
fn sources_set_enabled(state: State<AppState>, id: i64, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_source_enabled(&conn, id, enabled).map_err(|e| e.to_string())
}

/// Escaneo de todas las fuentes habilitadas.
///
/// `async` + `spawn_blocking`: el recorrido de disco sale del hilo principal (antes congelaba
/// la ventana) y el `Mutex` de SQLite solo se retiene para persistir cada fuente, no durante
/// el walk. Emite `scan_progress` (throttled) y `scan_finished`. Ver C.0_2 §3.
#[tauri::command]
async fn scan_run(app: AppHandle, task_id: Option<String>) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let tarea = task_id.unwrap_or_else(|| "scan".into());
        state.olvidar_cancelacion(&tarea);
        let sources: Vec<library::Source> = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            library::list_sources(&conn).map_err(|e| e.to_string())?
        }
        .into_iter()
        .filter(|s| s.enabled)
        .collect();

        let total = sources.len();
        let mut sum = ScanSummary::default();
        for (i, s) in sources.iter().enumerate() {
            if state.cancelada(&tarea) {
                sum.cancelada = true;
                break;
            }
            let mut found = 0usize;
            let mut last_emit = Instant::now() - Duration::from_secs(1);
            let items = sources::discover(&s.kind, &s.path, &mut |current| {
                found += 1;
                // Throttle: en bibliotecas grandes un evento por carpeta inundaría el WebView.
                if last_emit.elapsed() >= Duration::from_millis(80) {
                    last_emit = Instant::now();
                    let _ = app.emit(
                        "scan_progress",
                        ScanProgress {
                            source_index: i + 1,
                            source_total: total,
                            source_path: s.path.clone(),
                            current: current.to_string(),
                            found,
                        },
                    );
                }
                !state.cancelada(&tarea)
            });
            if state.cancelada(&tarea) {
                sum.cancelada = true;
            }
            match items {
                None => sum.skipped.push(s.path.clone()),
                Some(items) => {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    let st = sources::persist(&conn, s, &items).map_err(|e| e.to_string())?;
                    sum.scanned += st.found;
                    sum.added += st.added;
                    sum.updated += st.updated;
                    sum.excluded += st.skipped;
                    sum.uninstalled += st.uninstalled;
                }
            }
        }
        state.olvidar_cancelacion(&tarea);
        let _ = app.emit("scan_finished", sum.clone());
        Ok(sum)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Lista el grid aplicando los filtros combinables (guión F.0_1).
#[tauri::command]
fn library_list(
    state: State<AppState>,
    filter: Option<library::LibraryFilter>,
) -> Result<Vec<library::GameCard>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list_games(&conn, &filter.unwrap_or_default()).map_err(|e| e.to_string())
}

/// Contadores por estado para la sidebar (instalados / histórico / deseados / excluidos).
#[tauri::command]
fn state_counts(
    state: State<AppState>,
    recent_days: Option<i64>,
) -> Result<library::StateCounts, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::state_counts(&conn, recent_days.unwrap_or(14)).map_err(|e| e.to_string())
}

/// Plataformas y orígenes presentes en la biblioteca (desplegables de filtros).
#[tauri::command]
fn platforms_list(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list_platforms(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn origins_list(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list_origins(&conn).map_err(|e| e.to_string())
}

/// Cambia el estado de un elemento: excluir, restaurar, marcar como deseado…
#[tauri::command]
fn game_set_state(
    state: State<AppState>,
    id: i64,
    new_state: String,
) -> Result<(), String> {
    if !library::is_valid_state(&new_state) {
        return Err(format!("estado desconocido: {new_state}"));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_state(&conn, id, &new_state).map_err(|e| e.to_string())
}

/// ¿Hay que preguntar "launcher o juego directo" antes de lanzar? (guión C.0_3)
#[tauri::command]
fn launch_options(
    state: State<AppState>,
    id: i64,
    executable_id: Option<i64>,
) -> Result<Option<library::LaunchOptions>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::get_launch_options(&conn, id, executable_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn game_get(state: State<AppState>, id: i64) -> Result<Option<library::GameDetail>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::get_game(&conn, id).map_err(|e| e.to_string())
}

/// Lanza un juego siguiendo su Procedimiento y **registra el lanzamiento** (guión A §9.6).
///
/// `mode` (guión C.0_3): `"game"` = solo el juego · `"client"` = solo el launcher ·
/// ausente = comportamiento clásico (cliente y después juego). `remember` guarda la elección
/// para no volver a preguntar por este juego.
/// `async`: la espera del cliente previo ya no bloquea el hilo principal.
#[tauri::command]
async fn game_launch(
    app: AppHandle,
    id: i64,
    executable_id: Option<i64>,
    mode: Option<String>,
    remember: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let modo = library::LaunchMode::from_str(mode.as_deref());
        let plan = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            library::get_launch(&conn, id, executable_id).map_err(|e| e.to_string())?
        };
        let plan = plan.ok_or("El elemento ya no existe en la biblioteca.")?;

        let recordar = |modo: library::LaunchMode| -> Result<(), String> {
            if remember.unwrap_or(false) && modo != library::LaunchMode::Auto {
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                library::remember_launch_mode(&conn, id, modo).map_err(|e| e.to_string())?;
            }
            Ok(())
        };

        // "Solo el launcher": se abre el cliente y se termina — NO se lanza el juego. Este
        // era el fallo de Path of Titans: el launcher actualizaba con el juego ya abierto.
        if modo == library::LaunchMode::ClientOnly {
            match plan.client.as_deref().filter(|c| !c.trim().is_empty()) {
                Some(client) => launcher::launch_exe(client, plan.client_args.as_deref(), None)
                    .map(|_| ())
                    .map_err(|e| format!("launcher: {e}"))?,
                // Un juego de Steam sin ruta de cliente: abrir la ficha en Steam es lo que
                // el usuario espera de "abrir solo el launcher".
                None if plan.store.as_deref() == Some("steam") => {
                    let appid = plan.store_id.as_deref().unwrap_or_default();
                    launcher::abrir_uri(&format!("steam://nav/games/details/{appid}"))
                        .map_err(|e| format!("Steam: {e}"))?;
                }
                None => return Err("Este elemento no tiene launcher configurado.".into()),
            }
            recordar(modo)?;
            return Ok(());
        }

        // Juego de tienda: se lanza por el cliente (plan §4.3). Es más robusto que el .exe
        // directo, que puede fallar por DRM o por una actualización pendiente.
        if let (Some("steam"), Some(appid)) = (plan.store.as_deref(), plan.store_id.as_deref()) {
            launcher::abrir_uri(&steam::uri_lanzamiento(appid)).map_err(|e| e.to_string())?;
            recordar(modo)?;
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            return library::log_launch(&conn, id, None, &steam::uri_lanzamiento(appid))
                .map_err(|e| e.to_string());
        }

        let exe = plan.exe.as_deref().ok_or(
            "Este elemento no tiene ejecutable asignado. Ábrelo y usa Editar para indicar la ruta.",
        )?;
        // Solo el modo clásico arrastra el cliente por delante.
        if modo == library::LaunchMode::Auto && plan.client_first {
            if let Some(client) = plan.client.as_deref() {
                launcher::launch_client_and_wait(
                    client,
                    plan.client_args.as_deref(),
                    Duration::from_secs(3),
                )
                .map_err(|e| format!("cliente: {e}"))?;
            }
        }
        launcher::launch_exe(exe, plan.args.as_deref(), None).map_err(|e| e.to_string())?;
        recordar(modo)?;
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        library::log_launch(&conn, id, plan.executable_id, exe).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn game_update(
    state: State<AppState>,
    id: i64,
    title: String,
    platform: String,
    exe_path: Option<String>,
    exe_args: Option<String>,
    client_path: Option<String>,
    client_args: Option<String>,
    client_first: bool,
    players_min: Option<i64>,
    players_max: Option<i64>,
    launch_mode: Option<String>,
    steam_account: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::update_game(
        &conn,
        id,
        &library::GameEdit {
            title: &title,
            platform: &platform,
            exe_path: exe_path.as_deref(),
            exe_args: exe_args.as_deref(),
            client_path: client_path.as_deref(),
            client_args: client_args.as_deref(),
            client_first,
            players_min,
            players_max,
            launch_mode: launch_mode.as_deref(),
            steam_account: steam_account.as_deref(),
        },
    )
    .map_err(|e| e.to_string())
}

/// Resultado de buscar carátulas (guión D.0_1).
#[derive(Clone, Default, serde::Serialize)]
struct ArtSummary {
    /// Elementos que estaban sin carátula al empezar.
    pendientes: usize,
    /// Resueltas con una imagen que ya estaba en la carpeta del juego.
    de_carpeta: usize,
    /// Resueltas con la caché local de Steam.
    de_steam: usize,
    /// Resueltas buscando el nombre en la tienda de Steam (guión D.0_3).
    de_steam_web: usize,
    /// Resueltas con SteamGridDB.
    de_sgdb: usize,
    /// Resueltas con RAWG.
    de_rawg: usize,
    /// Resueltas con el icono del ejecutable, que es **el último recurso**.
    de_icono: usize,
    /// Siguen sin nada; la UI puede tirar del CDN si está activado.
    sin_resolver: usize,
    /// Juegos a los que se les ha deducido el appid de Steam por su carpeta.
    appids_deducidos: usize,
    /// El usuario lo paró a medias; lo resuelto hasta ese punto se conserva.
    cancelada: bool,
}

/// Busca carátulas recorriendo la cascada completa (guiones D.0_1, D.0_2 y **D.0_3**):
///
/// 1. lo que Steam ya tiene en disco —arte propio del usuario y caché del cliente—;
/// 2. **fuentes remotas por nombre**: tienda de Steam, SteamGridDB, RAWG;
/// 3. el **icono del `.exe`**, que pasa a ser el último recurso y no el tercero.
///
/// Ese reordenado es la mitad del arreglo: el icono no era una mala fuente, era una mala
/// **prioridad**. Acertaba siempre, así que ganaba antes de que nadie buscara un póster de
/// verdad, y 136 de 197 juegos acababan con un icono estirado a 150×225.
///
/// `async` + `spawn_blocking`: lee ficheros y ahora también la red; no debe congelar la ventana.
#[tauri::command]
async fn art_fetch(
    app: AppHandle,
    rehacer: Option<bool>,
    task_id: Option<String>,
) -> Result<ArtSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let tarea = task_id.unwrap_or_else(|| "art".into());
        state.olvidar_cancelacion(&tarea);
        let iconos = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("icons");

        let (pendientes, steam_dir, fuentes) = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            if rehacer.unwrap_or(false) {
                library::clear_art(&conn).map_err(|e| e.to_string())?;
            }
            // La carpeta de Steam sale de su fuente; si no hay, se intenta detectar.
            let dir = library::list_sources(&conn)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|s| s.kind == "steam" && !s.path.trim().is_empty())
                .map(|s| std::path::PathBuf::from(s.path))
                .or_else(steam::carpeta_steam);
            (
                library::list_sin_arte(&conn).map_err(|e| e.to_string())?,
                dir,
                fuentes_de_arte(&conn).map_err(|e| e.to_string())?,
            )
        };

        let mut sum = ArtSummary {
            pendientes: pendientes.len(),
            ..Default::default()
        };
        let total = pendientes.len();
        let mut ultimo = Instant::now() - Duration::from_secs(1);

        for (i, g) in pendientes.into_iter().enumerate() {
            let mut g = g;
            if state.cancelada(&tarea) {
                sum.cancelada = true;
                break;
            }
            if ultimo.elapsed() >= Duration::from_millis(80) {
                ultimo = Instant::now();
                let _ = app.emit(
                    "art_progress",
                    ScanProgress {
                        source_index: i + 1,
                        source_total: total,
                        source_path: String::new(),
                        current: g.title.clone(),
                        found: total - sum.sin_resolver,
                    },
                );
            }

            // El appid puede faltar si la biblioteca de Steam se añadió como carpeta normal.
            // Se deduce del `steamapps\common` que cuelga por encima y se guarda, para que
            // sirva también al lanzar y al filtrar (guión D.0_2 §2).
            if g.store_id.as_deref().unwrap_or("").is_empty() {
                if let Some(dir) = g.install_dir.as_deref() {
                    g.store_id = art::appid_desde_carpeta(std::path::Path::new(dir));
                    if let Some(a) = &g.store_id {
                        sum.appids_deducidos += 1;
                        let conn = state.db.lock().map_err(|e| e.to_string())?;
                        library::set_store(&conn, g.id, "steam", a).map_err(|e| e.to_string())?;
                    }
                }
            }

            match buscar_caratula(&g, steam_dir.as_deref(), &fuentes, &iconos) {
                Some((p, de)) => {
                    match de {
                        DeDonde::Carpeta => sum.de_carpeta += 1,
                        DeDonde::SteamLocal => sum.de_steam += 1,
                        DeDonde::Web(arte_web::Origen::SteamBuscado) => sum.de_steam_web += 1,
                        DeDonde::Web(arte_web::Origen::SteamGridDb) => sum.de_sgdb += 1,
                        DeDonde::Web(arte_web::Origen::Rawg) => sum.de_rawg += 1,
                        DeDonde::IconoExe => sum.de_icono += 1,
                    }
                    limpiar_arte_viejo(&iconos, g.id, &p);
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    library::set_art(&conn, g.id, &p).map_err(|e| e.to_string())?;
                }
                None => sum.sin_resolver += 1,
            }
        }
        state.olvidar_cancelacion(&tarea);
        let _ = app.emit("art_finished", sum.clone());
        Ok(sum)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Qué fuentes remotas de arte están activas, leído de los ajustes del core (guión D.0_3).
///
/// Las que piden clave están apagadas mientras no la haya, aunque su interruptor esté puesto:
/// sin clave la petición solo gasta una vuelta y un error.
fn fuentes_de_arte(conn: &rusqlite::Connection) -> rusqlite::Result<arte_web::Fuentes> {
    let activa = |k: &str, por_defecto: bool| -> rusqlite::Result<bool> {
        Ok(library::get_setting(conn, k)?
            .map(|v| v == "1")
            .unwrap_or(por_defecto))
    };
    let clave = |k: &str| -> rusqlite::Result<Option<String>> {
        Ok(library::get_setting(conn, k)?.filter(|v| !v.trim().is_empty()))
    };
    Ok(arte_web::Fuentes {
        steam: activa("art.fuente.steam", true)?,
        sgdb: if activa("art.fuente.sgdb", true)? {
            clave("art.sgdb_key")?
        } else {
            None
        },
        // RAWG viene apagada: devuelve una captura apaisada, que en una tarjeta vertical se
        // recorta mal. Quien la quiera, la enciende.
        rawg: if activa("art.fuente.rawg", false)? {
            clave("art.rawg_key")?
        } else {
            None
        },
    })
}

/// Lee las claves de arte que TimeTrack ya tenga configuradas, para no tener que copiarlas a
/// mano (guión D.0_3 §4). **Solo lee y devuelve**: no guarda nada. Quien decide si se importan
/// es el usuario, desde Ajustes — igual que con la recolocación de categorías, aquí no se
/// migra nada en silencio.
///
/// Devuelve `(sgdb, rawg)` con `null` en las que no estén puestas.
#[tauri::command]
fn timetrack_art_keys(state: State<AppState>) -> Result<serde_json::Value, String> {
    // La BD de TimeTrack vive junto a su ejecutable, que es la carpeta que ya se configura
    // para leerle el arte (`timetrack.dir`, guión T.0_1). No se añade un ajuste nuevo.
    let dir = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        library::get_setting(&conn, TT_DIR).map_err(|e| e.to_string())?
    }
    .filter(|d| !d.trim().is_empty())
    .ok_or("falta la carpeta de TimeTrack: ponla en Ajustes › TimeTrack")?;
    let ruta = std::path::Path::new(&dir).join("apptracker.db");
    if !ruta.is_file() {
        return Err(format!("no hay ninguna apptracker.db en {dir}"));
    }

    // Solo lectura: es la base de datos de **otra** app en marcha, y no nos toca escribirla.
    let tt = rusqlite::Connection::open_with_flags(
        &ruta,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| format!("{}: {e}", ruta.display()))?;

    let leer = |k: &str| -> Option<String> {
        tt.query_row("SELECT value FROM settings WHERE key = ?1", [k], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .filter(|v| !v.trim().is_empty())
    };
    Ok(serde_json::json!({
        "sgdb": leer("sgdb_api_key"),
        "rawg": leer("rawg_api_key"),
        "origen": ruta.display().to_string(),
    }))
}

/// Lee un ajuste del core (los de la interfaz viven en `localStorage`; ver guión H.0_1).
#[tauri::command]
fn setting_get(state: State<AppState>, key: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::get_setting(&conn, &key).map_err(|e| e.to_string())
}

#[tauri::command]
fn setting_set(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

/// Marca o desmarca un favorito. Independiente del estado del juego (guión L.0_2).
#[tauri::command]
fn game_set_favorite(state: State<AppState>, id: i64, favorite: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_favorite(&conn, id, favorite).map_err(|e| e.to_string())
}

/// Abre la carpeta del juego en el explorador de Windows (guión A.0_5 §2).
///
/// Se resuelve en el core, no en la UI: la card no lleva las rutas —solo si *hay* alguna— y
/// elegir entre carpeta y ejecutable exige mirar el disco, que es trabajo del lado de Rust.
#[tauri::command]
fn game_open_dir(state: State<AppState>, id: i64) -> Result<(), String> {
    let (dir, exe) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        library::rutas_de(&conn, id).map_err(|e| e.to_string())?
    };
    launcher::abrir_en_explorador(dir.as_deref(), exe.as_deref()).map_err(|e| e.to_string())
}

/// Fija la carátula de un juego a mano (`null` la quita y deja que mande la automática).
#[tauri::command]
fn game_set_cover(state: State<AppState>, id: i64, path: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_cover(&conn, id, path.as_deref()).map_err(|e| e.to_string())
}

/// De dónde ha salido una carátula, para contarlo en el resumen.
enum DeDonde {
    Carpeta,
    SteamLocal,
    Web(arte_web::Origen),
    IconoExe,
}

/// **La cascada de carátulas, en un solo sitio** (guión D.0_4 §2).
///
/// Antes vivía duplicada: una copia en `art_fetch` (el lote) y otra en `game_art_refresh` (el
/// botón de un juego). Cuando `D.0_3` añadió las fuentes de internet, solo se actualizó la
/// primera — así que «🔍 Buscar automáticamente» desde *Editar* seguía probando únicamente la
/// caché de Steam y el icono del `.exe`, y el icono seguía siendo el **segundo** salto en vez
/// del último. Dos caminos para lo mismo acaban siempre divergiendo; ahora hay uno.
///
/// No escribe en la base de datos: devuelve la ruta y quién la encontró.
fn buscar_caratula(
    g: &library::SinArte,
    steam_dir: Option<&std::path::Path>,
    fuentes: &arte_web::Fuentes,
    iconos: &std::path::Path,
) -> Option<(String, DeDonde)> {
    let appid = g.store_id.clone().filter(|a| !a.is_empty());

    // 1. Una imagen que ya esté en la carpeta del juego: gratis y casi siempre la correcta.
    if let Some(p) = g
        .install_dir
        .as_deref()
        .and_then(|d| sources::find_cover(std::path::Path::new(d)))
    {
        return p.to_str().map(|s| (s.to_string(), DeDonde::Carpeta));
    }
    // 2. Lo que Steam ya tiene en disco, incluido el arte propio del usuario.
    if let (Some(a), Some(s)) = (appid.as_deref(), steam_dir) {
        if let Some(p) = art::caratula_steam_local(s, a) {
            return p.to_str().map(|s| (s.to_string(), DeDonde::SteamLocal));
        }
    }
    // 3. Internet, buscando por nombre.
    if fuentes.alguna() {
        let destino = iconos.join(format!("{}-web", g.id));
        if let Some((p, origen)) =
            arte_web::buscar_poster(&g.title, appid.as_deref(), fuentes, &destino)
        {
            return p.to_str().map(|s| (s.to_string(), DeDonde::Web(origen)));
        }
    }
    // 4. El icono del `.exe`. **El último**: acierta siempre, así que antes tapaba a los demás.
    g.exe_path
        .as_deref()
        .filter(|e| !e.is_empty())
        .and_then(|e| {
            art::icono_de_exe(
                std::path::Path::new(e),
                &iconos.join(format!("{}.ico", g.id)),
            )
        })
        .and_then(|p| p.to_str().map(|s| (s.to_string(), DeDonde::IconoExe)))
}

/// Borra los ficheros de arte que este juego tenía y ya no usa.
///
/// Hace falta porque la extensión la decide lo encontrado (`123.png`, `123.bmp`, `123-web.jpg`):
/// sin esto, cada «Rehacer» deja otra copia huérfana en `icons`.
fn limpiar_arte_viejo(iconos: &std::path::Path, id: i64, conservar: &str) {
    let Ok(dir) = std::fs::read_dir(iconos) else {
        return;
    };
    let conservar = std::path::Path::new(conservar);
    for e in dir.flatten() {
        let p = e.path();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if (stem == id.to_string() || stem == format!("{id}-web")) && p != conservar {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Vuelve a buscar la carátula de **un solo** juego, con la **misma** cascada que el lote.
/// Devuelve la ruta encontrada, o `None` si no hay nada.
#[tauri::command]
async fn game_art_refresh(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let iconos = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("icons");

        let (mut g, steam_dir, fuentes) = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            library::clear_art_one(&conn, id).map_err(|e| e.to_string())?;
            let g = library::sin_arte_uno(&conn, id)
                .map_err(|e| e.to_string())?
                .ok_or("El juego ya no existe")?;
            let steam = library::list_sources(&conn)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|s| s.kind == "steam" && !s.path.trim().is_empty())
                .map(|s| std::path::PathBuf::from(s.path))
                .or_else(steam::carpeta_steam);
            (
                g,
                steam,
                fuentes_de_arte(&conn).map_err(|e| e.to_string())?,
            )
        };

        // El appid puede faltar si la biblioteca de Steam se añadió como carpeta normal.
        if g.store_id.as_deref().unwrap_or("").is_empty() {
            if let Some(d) = g.install_dir.as_deref() {
                g.store_id = art::appid_desde_carpeta(std::path::Path::new(d));
                if let Some(a) = &g.store_id {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    library::set_store(&conn, id, "steam", a).map_err(|e| e.to_string())?;
                }
            }
        }

        let encontrada = buscar_caratula(&g, steam_dir.as_deref(), &fuentes, &iconos);
        if let Some((p, _)) = &encontrada {
            limpiar_arte_viejo(&iconos, id, p);
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            library::set_art(&conn, id, p).map_err(|e| e.to_string())?;
        }
        Ok(encontrada.map(|(p, _)| p))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pantallas conectadas, en píxeles **físicos** y con su nombre de verdad (guión E.0_4 fase 1).
///
/// Vive en el core y no se apoya en la ventana: la lista hace falta también con el hub cerrado,
/// que es el caso de entrar en Modo Sofá desde el mando.
#[cfg(windows)]
#[tauri::command]
fn monitors_list() -> Vec<pantallas::Pantalla> {
    pantallas::listar()
}

/// Lleva la ventana a pantalla completa **en un monitor concreto**, o la devuelve (E.0_4 fase 2).
///
/// Hay que moverla **antes** de ponerla a pantalla completa: el sistema la expande en el monitor
/// donde esté, no en el que se le pida. Y la posición va en píxeles físicos, que es la única
/// medida que cuadra cuando el destino tiene otra escala —aquí el televisor va al 250 %—.
///
/// Esta fase **no toca la configuración de pantallas**: solo coloca el hub. Cambiar la principal
/// o apagar las demás es la fase 3, y se preguntará antes.
#[cfg(windows)]
#[tauri::command]
fn sofa_set(app: AppHandle, activo: bool, monitor: Option<String>) -> Result<(), String> {
    let w = app
        .get_webview_window("main")
        .ok_or("no hay ventana que mover")?;
    if !activo {
        w.set_fullscreen(false).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let pantallas = pantallas::listar();
    let destino = monitor
        .as_deref()
        .and_then(|id| pantallas.iter().find(|p| p.id == id))
        .or_else(|| pantallas.iter().find(|p| p.primaria))
        .ok_or("no se ha encontrado ninguna pantalla")?;

    // Salir primero: pedir pantalla completa estando ya en ella ignora el cambio de monitor.
    w.set_fullscreen(false).map_err(|e| e.to_string())?;
    w.set_position(tauri::PhysicalPosition::new(destino.x + 40, destino.y + 40))
        .map_err(|e| e.to_string())?;
    w.set_fullscreen(true).map_err(|e| e.to_string())?;
    let _ = w.set_focus();
    Ok(())
}

/// Botón que entra en Modo Sofá al mantenerlo. Se guarda en el core porque lo lee el **núcleo**,
/// que es quien sondea el mando con la ventana cerrada (guión E.0_4 §4.1 y H.0_3 §2.2).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct MandoConfig {
    /// Escuchar el mando desde el núcleo. Apagado, no hay sondeo ninguno.
    escucha: bool,
    /// Botón del gesto, por índice del *Standard Gamepad*.
    boton: u32,
    /// `mantener` · `pulsar` · `doble`.
    gesto: String,
    /// Milisegundos del mantenido.
    mantener_ms: u32,
    /// El botón Xbox trae el hub al frente.
    xbox_trae: bool,
    /// Exigir **dos pulsaciones seguidas** del botón Xbox (dentro de 1 s).
    #[serde(default)]
    xbox_doble: bool,
}

impl Default for MandoConfig {
    fn default() -> Self {
        Self {
            escucha: true,
            boton: 8, // SELECT
            gesto: "mantener".into(),
            mantener_ms: 1000,
            xbox_trae: true,
            xbox_doble: false,
        }
    }
}

const MANDO_CONFIG: &str = "mando.config";

#[cfg(windows)]
fn aplicar_config_mando(c: &MandoConfig) {
    use std::sync::atomic::Ordering::Relaxed;
    mando::ESCUCHA.store(c.escucha, Relaxed);
    mando::BOTON_SOFA.store(c.boton, Relaxed);
    mando::MANTENER_MS.store(c.mantener_ms, Relaxed);
    mando::XBOX_TRAE.store(c.xbox_trae, Relaxed);
    mando::XBOX_DOBLE.store(c.xbox_doble, Relaxed);
    mando::GESTO.store(
        match c.gesto.as_str() {
            "pulsar" => 1,
            "doble" => 2,
            _ => 0,
        },
        Relaxed,
    );
}

#[cfg(windows)]
#[tauri::command]
fn mando_config(app: AppHandle) -> MandoConfig {
    ajuste(&app, MANDO_CONFIG)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

#[cfg(windows)]
#[tauri::command]
fn mando_config_set(app: AppHandle, config: MandoConfig) -> Result<(), String> {
    aplicar_config_mando(&config);
    let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    poner_ajuste(&app, MANDO_CONFIG, &json);
    Ok(())
}

/// Arranca el sondeo del mando en el núcleo y reparte sus gestos (guión E.0_4 §9).
///
/// Aquí solo se decide **lo que el núcleo puede decidir solo**: si no hay ventana, el botón Xbox
/// la trae. Todo lo demás —entrar o salir del Modo Sofá, tabular al menú superpuesto— depende de
/// en qué estado esté la interfaz, así que se le pasa a ella y que decida.
#[cfg(windows)]
fn escuchar_mando(app: &AppHandle) {
    aplicar_config_mando(&mando_config(app.clone()));
    let app = app.clone();
    mando::escuchar(move |g| {
        let hay_ventana = app.get_webview_window("main").is_some();
        match g {
            // El botón Xbox depende de **dónde está el hub**, no solo de si existe. Son cuatro
            // casos, y al principio solo se distinguían dos: tener la ventana creada pero detrás
            // de un juego no es tenerla a mano, y ahí no pasaba nada.
            mando::Gesto::Xbox => {
                if !hay_ventana || !service::es_primer_plano(&app) {
                    // No está, o está detrás: tráela. `show_hub` crea o reenfoca, y se salta
                    // el bloqueo de primer plano de Windows.
                    service::show_hub(&app);
                } else if !service::volver_al_anterior() {
                    // Está delante. Si hay algo detrás a lo que volver, el mismo botón aparta el
                    // hub —eso lo hace un **interruptor** y no un camino de ida—. Y si no hay
                    // nada, el botón significa otra cosa que decide la interfaz (E.0_4 §9).
                    let _ = app.emit("mando_xbox", ());
                }
            }
            // Mantener con el hub cerrado: se abre y se deja que la interfaz siga el gesto.
            mando::Gesto::Completado => {
                if !hay_ventana {
                    service::show_hub(&app);
                }
                let _ = app.emit("mando_sofa", ());
            }
            // El progreso solo tiene sentido si hay dónde pintarlo.
            mando::Gesto::Progreso(p) if hay_ventana => {
                let _ = app.emit("mando_sofa_progreso", p);
            }
            // Pulsación corta del botón reservado: es la interfaz quien sabe qué acción tiene
            // asignada (de fábrica, la guía de botones).
            mando::Gesto::Corta if hay_ventana => {
                let _ = app.emit("mando_sofa_corto", ());
            }
            _ => {}
        }
    });
}

/// Dónde se guarda la disposición de pantallas anterior, mientras el Modo Sofá la tiene cambiada.
const SOFA_TOPOLOGIA: &str = "sofa.topologia";
/// `1` cuando el usuario ha confirmado que ve bien la pantalla nueva.
const SOFA_CONFIRMADO: &str = "sofa.confirmado";
/// Lo que se espera a que el usuario confirme antes de deshacerlo todo (guión E.0_4 §5.5).
///
/// 20 y no 15: el caso real es un televisor que acaba de recibir señal. Entre que sincroniza,
/// que la persona lo mira y que coge el mando se van unos segundos que no son suyos, y quedarse
/// corto significa deshacer un cambio que había funcionado.
const SEGUROS_SEGUNDOS: u64 = 20;

fn ajuste(app: &AppHandle, clave: &str) -> Option<String> {
    let estado = app.state::<AppState>();
    let conn = estado.db.lock().ok()?;
    library::get_setting(&conn, clave).ok().flatten()
}

fn poner_ajuste(app: &AppHandle, clave: &str, valor: &str) {
    let estado = app.state::<AppState>();
    if let Ok(conn) = estado.db.lock() {
        let _ = library::set_setting(&conn, clave, valor);
    }
}

/// Devuelve el escritorio a como estaba, si es que lo habíamos cambiado.
///
/// Es **idempotente** a propósito: se llama al salir del modo, desde el temporizador de
/// seguridad y al arrancar. Que se llame dos veces no puede romper nada.
fn sofa_deshacer(app: &AppHandle) -> Result<(), String> {
    let Some(foto) = ajuste(app, SOFA_TOPOLOGIA).filter(|f| !f.trim().is_empty()) else {
        return Ok(());
    };
    let r = pantallas::restaurar(&foto);
    poner_ajuste(app, SOFA_TOPOLOGIA, "");
    poner_ajuste(app, SOFA_CONFIRMADO, "");
    r
}

/// Aplica el reparto de pantallas del Modo Sofá (guión E.0_4 fase 3).
///
/// Guarda **antes** la disposición actual, y arranca una cuenta atrás: si el usuario no confirma
/// en 20 segundos, se deshace sola. Eso cubre el caso que de verdad da miedo — elegir el
/// televisor estando apagado, con el HDMI puesto, y quedarse sin escritorio visible: Windows lo
/// sigue viendo conectado, así que nadie más va a avisar.
///
/// El temporizador vive en el **núcleo**, no en la interfaz: si lo que se rompe es justamente la
/// ventana, un temporizador dentro de ella no serviría de nada.
#[cfg(windows)]
#[tauri::command]
fn sofa_aplicar(
    app: AppHandle,
    reparto: pantallas::Reparto,
    monitor: String,
) -> Result<Vec<pantallas::Pantalla>, String> {
    // Si quedaba algo pendiente de una vez anterior, se deshace antes de volver a tocar nada.
    let _ = sofa_deshacer(&app);

    let antes = pantallas::aplicar(reparto, &monitor)?;
    if reparto == pantallas::Reparto::Ninguna {
        return Ok(pantallas::listar());
    }

    poner_ajuste(&app, SOFA_TOPOLOGIA, &antes);
    poner_ajuste(&app, SOFA_CONFIRMADO, "");

    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(SEGUROS_SEGUNDOS));
        if ajuste(&app2, SOFA_CONFIRMADO).as_deref() == Some("1") {
            return;
        }
        let _ = sofa_deshacer(&app2);
        // La interfaz, si sigue viva, tiene que enterarse de que se ha deshecho.
        let _ = app2.emit("sofa_revertido", ());
    });

    Ok(pantallas::listar())
}

/// «Sí, se ve bien»: para la cuenta atrás. La disposición anterior se conserva para el final.
#[cfg(windows)]
#[tauri::command]
fn sofa_confirmar(app: AppHandle) {
    poner_ajuste(&app, SOFA_CONFIRMADO, "1");
}

/// Devuelve las pantallas a como estaban. Se llama al salir del Modo Sofá.
#[cfg(windows)]
#[tauri::command]
fn sofa_restaurar(app: AppHandle) -> Result<Vec<pantallas::Pantalla>, String> {
    sofa_deshacer(&app)?;
    Ok(pantallas::listar())
}

/// Mueve el cursor del sistema (stick derecho del mando). Ver guión E.0_3.
#[tauri::command]
fn cursor_move(dx: i32, dy: i32) {
    cursor::mover(dx, dy);
}

/// Pulsa/suelta un botón del ratón. Los dos flancos, para poder arrastrar.
#[tauri::command]
fn cursor_button(left: bool, pressed: bool) {
    cursor::boton(left, pressed);
}

/// Añade Steam como fuente detectándolo solo. Devuelve la ruta encontrada.
#[tauri::command]
fn steam_detect(state: State<AppState>) -> Result<Option<String>, String> {
    let Some(dir) = steam::carpeta_steam() else {
        return Ok(None);
    };
    let ruta = dir.to_string_lossy().to_string();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // Si ya existe una fuente Steam, no se duplica.
    let ya = library::list_sources(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|s| s.kind == "steam");
    if !ya {
        library::add_source(&conn, "steam", &ruta, None).map_err(|e| e.to_string())?;
    }
    Ok(Some(ruta))
}

#[tauri::command]
fn game_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::delete_game(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn executable_add(
    state: State<AppState>,
    game_id: i64,
    label: String,
    path: String,
    args: Option<String>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::add_executable(&conn, game_id, &label, &path, args.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn executable_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::delete_executable(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_unassign(
    state: State<AppState>,
    game_id: i64,
    category_id: i64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::unassign_category(&conn, game_id, category_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn categories_list(state: State<AppState>) -> Result<Vec<library::Category>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list_categories(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_create(
    state: State<AppState>,
    name: String,
    parent_id: Option<i64>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::create_category(&conn, &name, parent_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_rename(state: State<AppState>, id: i64, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::rename_category(&conn, id, name).map_err(|e| match e {
        // El UNIQUE de `category.name` es la regla; el mensaje crudo de SQLite no ayuda.
        rusqlite::Error::SqliteFailure(_, _) => format!("Ya existe una categoría «{name}»"),
        other => other.to_string(),
    })
}

#[tauri::command]
fn category_set_parent(
    state: State<AppState>,
    id: i64,
    parent_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_category_parent(&conn, id, parent_id).map_err(|e| e.to_string())
}

/// Sube (`-1`) o baja (`+1`) una categoría entre sus hermanas (A3: orden manual).
#[tauri::command]
fn category_move(state: State<AppState>, id: i64, delta: i64) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::move_category(&conn, id, delta).map_err(|e| e.to_string())
}

/// Añade las categorías por defecto que falten. Devuelve los nombres creados.
#[tauri::command]
fn categories_add_defaults(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::ensure_default_categories(&conn).map_err(|e| e.to_string())
}

/// Categorías que encajan mejor en otro eje. **Solo propone**; no cambia nada.
#[tauri::command]
fn categories_suggest_moves(
    state: State<AppState>,
) -> Result<Vec<library::Recolocacion>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::sugerir_recolocacion(&conn).map_err(|e| e.to_string())
}

/// Aplica solo las recolocaciones que el usuario haya confirmado.
#[tauri::command]
fn categories_apply_moves(state: State<AppState>, ids: Vec<i64>) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::aplicar_recolocacion(&conn, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::delete_category(&conn, id).map_err(|e| e.to_string())
}



/// Categorías asignadas a una fuente entera (F10).
#[tauri::command]
fn source_categories(state: State<AppState>, source_id: i64) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::source_categories(&conn, source_id).map_err(|e| e.to_string())
}

/// Asigna una categoría a toda una fuente. `retroactivo` la aplica además a los juegos que
/// ya estaban, **incluido el histórico**. Devuelve cuántos se han etiquetado.
#[tauri::command]
fn source_category_assign(
    state: State<AppState>,
    source_id: i64,
    category_id: i64,
    retroactivo: bool,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::assign_source_category(&conn, source_id, category_id, retroactivo)
        .map_err(|e| e.to_string())
}

/// Quita una categoría de una fuente; con `limpiar`, también de sus juegos.
#[tauri::command]
fn source_category_unassign(
    state: State<AppState>,
    source_id: i64,
    category_id: i64,
    limpiar: bool,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::unassign_source_category(&conn, source_id, category_id, limpiar)
        .map_err(|e| e.to_string())
}

/// Confirmación previa al lanzar para los juegos de una categoría (C8, «Launcher SpaceWar»).
#[tauri::command]
fn category_set_prompt(
    state: State<AppState>,
    id: i64,
    prompt: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::set_category_prompt(&conn, id, prompt.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_assign(
    state: State<AppState>,
    game_id: i64,
    category_id: i64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::assign_category(&conn, game_id, category_id).map_err(|e| e.to_string())
}

// ── TimeTrack (M2 · guión T.0_1) ────────────────────────────────────────────────────

const TT_BASE: &str = "timetrack.base";
const TT_DIR: &str = "timetrack.dir";

#[derive(Clone, serde::Serialize)]
struct TimeTrackConfig {
    base: String,
    dir: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct TimeTrackSync {
    estado: timetrack::Estado,
    apps: usize,
    con_tiempo: usize,
    con_arte: usize,
}

#[tauri::command]
fn timetrack_config(state: State<AppState>) -> Result<TimeTrackConfig, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(TimeTrackConfig {
        base: library::get_setting(&conn, TT_BASE)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| timetrack::BASE_POR_DEFECTO.to_string()),
        dir: library::get_setting(&conn, TT_DIR)
            .map_err(|e| e.to_string())?
            .filter(|d| !d.is_empty()),
    })
}

#[tauri::command]
fn timetrack_set_config(
    state: State<AppState>,
    base: Option<String>,
    dir: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if let Some(b) = base {
        let b = b.trim().trim_end_matches('/');
        library::set_setting(
            &conn,
            TT_BASE,
            if b.is_empty() {
                timetrack::BASE_POR_DEFECTO
            } else {
                b
            },
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(d) = dir {
        library::set_setting(&conn, TT_DIR, d.trim()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Sincroniza tiempo jugado y arte desde TimeTrack. `async`: la red no bloquea la ventana.
/// **Nunca** arranca ni mata TimeTrack: si no responde, se informa y ya (plan §4.5).
#[tauri::command]
async fn timetrack_sync(app: AppHandle) -> Result<TimeTrackSync, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (base, dir) = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            (
                library::get_setting(&conn, TT_BASE)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| timetrack::BASE_POR_DEFECTO.to_string()),
                library::get_setting(&conn, TT_DIR)
                    .map_err(|e| e.to_string())?
                    .filter(|d| !d.is_empty()),
            )
        };

        let estado = timetrack::estado(&base);
        let timetrack::Estado::Conectado { profile_id, .. } = &estado else {
            return Ok(TimeTrackSync {
                estado,
                apps: 0,
                con_tiempo: 0,
                con_arte: 0,
            });
        };

        // La parte lenta (HTTP) va fuera del lock.
        // El arte de TimeTrack es **opcional**: la cascada propia (guión D.0_2) resuelve ya
        // la biblioteca sin APIs, y la integración con TimeTrack está en revisión. Solo se
        // pide si se activa en Ajustes, y aun así nunca pisa una carátula existente.
        let usar_arte = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            library::get_setting(&conn, "timetrack.arte")
                .map_err(|e| e.to_string())?
                .as_deref()
                == Some("1")
        };
        let remotas = timetrack::apps_del_perfil(&base, *profile_id, dir.as_deref());
        let mapa: std::collections::HashMap<String, library::DatosRemotos> = remotas
            .iter()
            .map(|a| {
                // Para la card preferimos el póster (vertical, tipo carátula); si no, el
                // banner; y como último recurso el icono del exe, que para emuladores y
                // apps sueltas suele ser justo lo que se quiere ver.
                let arte = if usar_arte {
                    a.poster
                        .clone()
                        .or_else(|| a.banner.clone())
                        .or_else(|| a.icon.clone())
                } else {
                    None
                };
                (
                    a.exe_name.clone(),
                    library::DatosRemotos {
                        total_secs: a.total_secs,
                        arte,
                        app_id: a.app_id,
                    },
                )
            })
            .collect();

        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let (con_tiempo, con_arte) =
            library::sync_timetrack(&conn, &mapa).map_err(|e| e.to_string())?;
        Ok(TimeTrackSync {
            estado,
            apps: remotas.len(),
            con_tiempo,
            con_arte,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ventana que muestra las **gráficas de TimeTrack** de un juego concreto (guión T.0_2).
const TT_VENTANA: &str = "timetrack";

/// Abre el panel de detalle de TimeTrack para este juego: su gráfica semanal, la mensual, las
/// sesiones y el resumen.
///
/// **No se dibujan aquí.** TimeTrack ya tiene ese panel hecho, con sus gráficas, su zoom y su
/// histórico de sesiones; replicarlo sería mantener dos veces lo mismo y quedarse atrás cada vez
/// que él lo mejore. Se abre **su** dashboard en una ventana aparte y se le dice qué app mirar.
///
/// Y se le dice de la única forma que admite: su dashboard expone `openDetail(appId)` como función
/// global, pero **no acepta ningún parámetro en la URL** para ello (lo único que hay es
/// `/apps-page?highlight=<id>`, que resalta la fila en la lista pero no abre las gráficas). Así
/// que se inyecta una llamada que espera a que la función exista.
#[tauri::command]
async fn timetrack_open_app(app: AppHandle, id: i64) -> Result<(), String> {
    let (base, app_id) = {
        let estado = app.state::<AppState>();
        let conn = estado.db.lock().map_err(|e| e.to_string())?;
        let base = library::get_setting(&conn, TT_BASE)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| timetrack::BASE_POR_DEFECTO.to_string());
        let app_id: Option<i64> = conn
            .query_row(
                "SELECT tt_app_id FROM playtime_cache WHERE game_id = ?1",
                [id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        (base, app_id)
    };
    let Some(app_id) = app_id else {
        return Err("este juego no está correlacionado con ninguna app de TimeTrack: \
                    sincroniza en Ajustes › TimeTrack"
            .into());
    };

    // Se comprueba **antes** de abrir la ventana. Si TimeTrack no responde, lo que saldría es un
    // error de red dentro de un WebView en blanco, que no dice nada; aquí se puede decir qué pasa.
    // Y se respeta la regla de siempre: el hub no lo arranca (plan §4.5).
    if matches!(timetrack::estado(&base), timetrack::Estado::NoResponde) {
        return Err(format!(
            "TimeTrack no responde en {base}: ábrelo para ver sus gráficas"
        ));
    }

    // Esto es un WebView aparte, con su coste. Por eso se abre **solo a petición** y se reutiliza
    // la misma ventana para los juegos siguientes, en vez de una por juego.
    //
    // El guion de abajo salió de **leer su dashboard**, porque la primera versión —esperar a que
    // `openDetail` existiera y llamarla— abría la ventana pero no el panel. Dos razones, y
    // ninguna se ve desde fuera:
    //
    // 1. `openDetail(id)` empieza con `lastSummary.find(a => a.id === id)` y **sale sin hacer
    //    nada** si no lo encuentra. Esperar a que exista la *función* no sirve: existe desde que
    //    se parsea el script, mucho antes de que haya datos.
    // 2. `lastSummary` sale de `summary?days=7`, y 7 días es su valor por defecto. Un juego que no
    //    se haya tocado esta semana **no está en esa lista**, así que esperar tampoco bastaba:
    //    hay que mover su filtro de rango. Para eso se pulsa su propio botón «All» (`#allBtn`),
    //    que es exactamente lo que haría una persona.
    //
    // Se amplía **solo si hace falta**: si el juego ya aparece en el rango que el usuario tuviera
    // puesto en TimeTrack, no se le cambia la vista.
    //
    // Y `lastSummary` se lee como identificador suelto, no como `window.lastSummary`: está
    // declarado con `let` en el script de la página, y `let` **no crea propiedad en `window`**.
    let abrir = format!(
        r#"(function () {{
          if (location.pathname !== "/") return;      // solo el dashboard tiene openDetail
          var id = {app_id};
          var ampliado = false, n = 0;
          function listo() {{
            return typeof lastSummary !== "undefined" && Array.isArray(lastSummary)
              && lastSummary.some(function (a) {{ return a.id === id; }});
          }}
          var t = setInterval(function () {{
            n++;
            if (typeof openDetail === "function" && listo()) {{
              clearInterval(t);
              openDetail(id);
              return;
            }}
            // Segundo y medio de gracia para su carga inicial; si a esas alturas no ha salido, es
            // que está fuera del rango y hay que pedirle todo el histórico.
            if (!ampliado && n > 15) {{
              ampliado = true;
              var b = document.getElementById("allBtn");
              if (b) b.click();
            }}
            if (n > 150) clearInterval(t);             // 15 s y se deja de insistir
          }}, 100);
        }})();"#
    );

    if let Some(w) = app.get_webview_window(TT_VENTANA) {
        // Ya abierta: no se recarga la página —perdería el estado y tardaría— solo se le pide
        // el otro juego. `initialization_script` no vale aquí: no hay documento nuevo.
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        w.eval(&abrir).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = tauri::Url::parse(&base).map_err(|e| format!("la dirección de TimeTrack no vale: {e}"))?;
    tauri::WebviewWindowBuilder::new(&app, TT_VENTANA, tauri::WebviewUrl::External(url))
        .title("TimeTrack — gráficas")
        .inner_size(1180.0, 820.0)
        .center()
        .focused(true)
        .initialization_script(&abrir)
        .build()
        .map_err(|e| format!("no se pudo abrir la ventana de TimeTrack: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Tiene que ir **el primero**. Sin esto, cada doble clic en el `.exe` arrancaba otro
        // core residente: se apilaban iconos de bandeja y ventanas, y la app parecía «no
        // abrirse» porque el proceso nuevo tapaba al viejo en vez de reutilizarlo.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Volver a lanzar el ejecutable = «quiero ver el hub».
            service::show_hub(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("app_data_dir");
            std::fs::create_dir_all(&dir).ok();
            let conn = library::open_db(&dir.join("library.db")).expect("open db");
            app.manage(AppState {
                db: Mutex::new(conn),
                canceladas: Mutex::new(std::collections::HashSet::new()),
            });
            service::setup_tray(app.handle())?;
            // Si la app murió con el escritorio reconfigurado por el Modo Sofá, se deshace ahora.
            // Un cierre sucio no puede dejar las pantallas cambiadas para siempre (E.0_4 §5.6).
            #[cfg(windows)]
            {
                let _ = sofa_deshacer(app.handle());
                escuchar_mando(app.handle());
            }
            // La ventana la crea **siempre** `show_hub`, nunca `tauri.conf.json`. Cuando la
            // declaraba la configuración había dos caminos distintos para crearla, y el del
            // arranque no aplicaba la geometría guardada ni la volvía a guardar al cerrar:
            // por eso H.0_2 no llegaba a funcionar nunca (`ventana.geometria` estaba vacío).
            service::show_hub(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            sources_add,
            sources_list,
            sources_remove,
            sources_set_enabled,
            scan_run,
            library_list,
            state_counts,
            platforms_list,
            origins_list,
            game_set_state,
            launch_options,
            game_get,
            game_launch,
            game_update,
            game_delete,
            executable_add,
            executable_delete,
            categories_list,
            category_create,
            category_rename,
            category_set_parent,
            category_move,
            categories_add_defaults,
            categories_suggest_moves,
            categories_apply_moves,
            category_delete,
            category_assign,
            category_unassign,
            source_categories,
            source_category_assign,
            source_category_unassign,
            category_set_prompt,
            steam_detect,
            art_fetch,
            setting_get,
            setting_set,
            timetrack_art_keys,
            game_set_favorite,
            game_open_dir,
            game_set_cover,
            game_art_refresh,
            task_cancel,
            cursor_move,
            cursor_button,
            monitors_list,
            sofa_set,
            sofa_aplicar,
            sofa_confirmar,
            sofa_restaurar,
            mando_config,
            mando_config_set,
            timetrack_config,
            timetrack_set_config,
            timetrack_sync,
            timetrack_open_app
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Cerrar la ventana NO cierra la app: el core sigue residente en la bandeja.
            // Solo salimos si se pide explícitamente (tray "Salir" → app.exit(code)).
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
