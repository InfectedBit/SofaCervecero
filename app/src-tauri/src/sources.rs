//! Source providers: descubren juegos (scan) y construyen su Procedimiento de lanzamiento.
//! Ver plan §5.3 y guión A (GameFetch). M1: `folder_library` + `app_entry` (local).

use crate::library;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Origen de juegos/apps.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    FolderLibrary, // carpeta con muchos juegos (subcarpeta ≈ juego)
    AppEntry,      // .exe suelto añadido por el usuario (Dolphin, shadPS4…)
    Steam,
    Epic,
    Gog,
    Ea,
    Ubisoft,
    Amazon,
    Luna,
    Emulator,
}

/// Un juego/app encontrado, **antes** de tocar la BD. Definido en `library` porque es el
/// contrato de entrada del `upsert`.
pub use crate::library::Discovered;

/// Resultado de escanear una fuente.
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceStats {
    pub found: usize,
    pub added: usize,
    pub updated: usize,
    /// Encontrados en disco pero **excluidos** por el usuario: se dejan intactos.
    pub skipped: usize,
    /// Ya no están en disco → pasan a `uninstalled` (histórico).
    pub uninstalled: usize,
}

/// Recorre el disco de una fuente y devuelve lo encontrado. **No abre la BD**, así que puede
/// ejecutarse sin retener el `Mutex` de SQLite (C.0_2 §3). `on_item` recibe el nombre de cada
/// elemento en curso, para reportar progreso.
///
/// `None` = la ruta de la fuente no es accesible (disco desconectado, carpeta borrada). Se
/// distingue de "0 encontrados" para **no** marcar toda la fuente como desaparecida.
///
/// `on_item` devuelve `false` para **abortar** el recorrido: así el usuario puede cancelar un
/// escaneo largo y se conserva lo encontrado hasta ese punto (guión J.0_1).
pub fn discover(
    kind: &str,
    path: &str,
    on_item: &mut dyn FnMut(&str) -> bool,
) -> Option<Vec<Discovered>> {
    let mut out = Vec::new();
    match kind {
        "app_entry" => {
            let exe = PathBuf::from(path);
            if !exe.is_file() {
                return Some(out); // el .exe concreto ya no está → sí es "desaparecido"
            }
            let title = exe
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or("App")
                .to_string();
            if !on_item(&title) {
                return Some(out);
            }
            out.push(Discovered {
                source_key: exe.to_str().unwrap_or(&title).to_string(),
                install_dir: exe.parent().and_then(|p| p.to_str()).map(String::from),
                cover_path: exe
                    .parent()
                    .and_then(find_cover)
                    .and_then(|p| p.to_str().map(String::from)),
                exe_path: exe.to_str().map(String::from),
                platform: "PC".into(),
                title,
                ..Default::default()
            });
        }
        "folder_library" => {
            let entries = std::fs::read_dir(path).ok()?;
            for e in entries.flatten() {
                let dir = e.path();
                if !dir.is_dir() {
                    continue;
                }
                let title = folder_name(&dir);
                if !on_item(&title) {
                    break; // cancelado: se devuelve lo encontrado hasta aquí
                }
                // Las carpetas sin exe reconocible se registran igualmente, con `exe_path`
                // vacío: antes se descartaban en silencio y el juego "desaparecía" sin que el
                // usuario pudiera corregirlo a mano (C.0_2 §4).
                let exe = find_main_exe(&dir, &normalize(&title));
                out.push(Discovered {
                    source_key: dir.to_str().unwrap_or(&title).to_string(),
                    install_dir: dir.to_str().map(String::from),
                    cover_path: find_cover(&dir).and_then(|p| p.to_str().map(String::from)),
                    exe_path: exe.and_then(|p| p.to_str().map(String::from)),
                    platform: "PC".into(),
                    title,
                    ..Default::default()
                });
            }
        }
        // Steam: lo instalado sale de sus propios ficheros, no de recorrer carpetas.
        // `path` vacío = autodetectar la instalación (guión B.0_1 §1).
        "steam" => {
            let raiz = if path.trim().is_empty() {
                crate::steam::carpeta_steam()?
            } else {
                let p = PathBuf::from(path);
                if !p.is_dir() {
                    return None; // ruta inaccesible: no marcar nada como desinstalado
                }
                p
            };
            for j in crate::steam::escanear(&raiz) {
                if !on_item(&j.nombre) {
                    break;
                }
                out.push(Discovered {
                    // Dedupe por appid: el juego es el mismo aunque cambie de biblioteca.
                    source_key: format!("steam:{}", j.appid),
                    install_dir: j.install_dir.to_str().map(String::from),
                    cover_path: find_cover(&j.install_dir)
                        .and_then(|p| p.to_str().map(String::from)),
                    // Sin exe: se lanza por `steam://rungameid/<appid>` (más robusto).
                    exe_path: None,
                    platform: "PC (Steam)".into(),
                    store: Some("steam".into()),
                    store_id: Some(j.appid.clone()),
                    title: j.nombre,
                });
            }
        }
        _ => {}
    }
    Some(out)
}

/// Persiste lo descubierto en una fuente: upsert + categoría por defecto + paso al histórico
/// de los que ya no están en disco (`uninstalled`; nunca se borra nada). Respeta los excluidos.
pub fn persist(
    conn: &Connection,
    source: &library::Source,
    items: &[Discovered],
) -> rusqlite::Result<SourceStats> {
    let mut st = SourceStats {
        found: items.len(),
        ..Default::default()
    };
    // Una fuente puede tener varias categorías (F10): se aplican todas a lo que traiga.
    let categorias = library::source_categories(conn, source.id)?;
    for it in items {
        let (id, outcome) = library::upsert_game(conn, source.id, it)?;
        match outcome {
            library::Upsert::Created => st.added += 1,
            library::Upsert::Updated => st.updated += 1,
            // Excluido: ni se actualiza ni se le asigna categoría.
            library::Upsert::Skipped => {
                st.skipped += 1;
                continue;
            }
        }
        // `_auto`: respeta lo que el usuario haya quitado a mano a ese juego concreto
        // (guión A.0_8). La regla de la fuente vale para los nuevos, no para deshacer
        // decisiones tomadas.
        for cat in &categorias {
            library::assign_category_auto(conn, id, *cat)?;
        }
    }
    let keys: Vec<String> = items.iter().map(|i| i.source_key.clone()).collect();
    st.uninstalled = library::mark_uninstalled(conn, source.id, &keys)?;
    Ok(st)
}

/// Escaneo completo en un solo paso. Solo para tests: la app usa `discover` + `persist` por
/// separado para no retener el lock de SQLite durante el recorrido de disco (ver `lib.rs`).
#[cfg(test)]
pub fn scan_all(conn: &Connection) -> rusqlite::Result<usize> {
    let mut count = 0usize;
    for s in library::list_sources(conn)? {
        if !s.enabled {
            continue;
        }
        if let Some(items) = discover(&s.kind, &s.path, &mut |_| true) {
            count += persist(conn, &s, &items)?.found;
        }
    }
    Ok(count)
}

fn folder_name(p: &Path) -> String {
    p.file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("Juego")
        .to_string()
}

/// Busca una carátula ya presente en la carpeta del juego — la fuente más barata de todas y
/// la que tiene prioridad sobre cualquier otra (guión D.0_2 §4).
///
/// Los nombres van **por orden de preferencia**, no por orden de aparición en el directorio:
/// si hay un `cover.png` y un `header.jpg`, gana el primero.
///
/// La usa también `art_fetch` (D.0_4): «Rehacer» limpia lo automático, y sin volver a mirar la
/// carpeta aquí se perderían las imágenes que había encontrado el escaneo hasta el siguiente.
pub fn find_cover(dir: &Path) -> Option<PathBuf> {
    // De más "carátula" a más "cualquier imagen del juego".
    const NAMES: &[&str] = &[
        "cover", "folder", "box", "boxart", "grid", "poster", "capsule",
        "library_600x900", "portrait", "header", "capsule_616x353", "logo", "icon",
    ];
    const EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "ico"];

    let carpeta = dir
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut candidatos: Vec<(usize, PathBuf)> = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let stem = p
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_lowercase();
        let ext = p
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !EXTS.contains(&ext.as_str()) {
            continue;
        }
        // Una imagen que se llama como la carpeta suele ser la carátula del juego.
        let rango = if stem == carpeta {
            0
        } else {
            match NAMES.iter().position(|n| *n == stem) {
                Some(i) => i + 1,
                None => continue,
            }
        };
        candidatos.push((rango, p));
    }
    candidatos.sort_by_key(|(r, _)| *r);
    candidatos.into_iter().next().map(|(_, p)| p)
}

/// Substrings que descartan un exe/carpeta como "no-juego".
const EXCLUDE: &[&str] = &[
    "unins", "vcredist", "vc_redist", "dxsetup", "dxwebsetup", "setup", "install",
    "crashreport", "crashhandler", "crashpad", "helper", "redist", "dotnet",
    "notification_helper", "handler", "cleanup", "launcher_helper",
    "diag", "report", "arm64", "prereq", "support",
];

fn is_excluded(name_lower: &str) -> bool {
    EXCLUDE.iter().any(|s| name_lower.contains(s))
}

/// Elige el exe principal de una carpeta de juego (recursión acotada + heurística).
fn find_main_exe(dir: &Path, name_key: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_exes(dir, 0, 6, &mut candidates);
    candidates.into_iter().min_by_key(|p| score(p, name_key))
}

fn collect_exes(dir: &Path, depth: usize, max: usize, out: &mut Vec<PathBuf>) {
    if depth > max {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let dl = p
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !is_excluded(&dl) {
                collect_exes(&p, depth + 1, max, out);
            }
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| x.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
        {
            let nl = p
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !is_excluded(&nl) {
                out.push(p);
            }
        }
    }
}

/// Menor score = mejor candidato.
fn score(p: &Path, name_key: &str) -> i64 {
    let fname = p.file_stem().and_then(|x| x.to_str()).unwrap_or("");
    let fnorm = normalize(fname);
    let plow = p.to_string_lossy().to_lowercase();
    let depth = p.components().count() as i64;
    let size_mb = (std::fs::metadata(p).map(|m| m.len()).unwrap_or(0) / (1024 * 1024)) as i64;
    let mut s = 0i64;
    // el nombre del exe coincide con / contiene el del juego
    if !name_key.is_empty()
        && fnorm.len() >= 3
        && (fnorm.contains(name_key) || name_key.contains(fnorm.as_str()))
    {
        s -= 1000;
    }
    // ubicaciones típicas de "shipping" (Unreal/Unity)
    if plow.contains("\\binaries\\win64")
        || plow.contains("/binaries/win64")
        || plow.contains("\\win64\\")
    {
        s -= 300;
    }
    if fnorm.contains("shipping") {
        s -= 200;
    }
    // la variante BattlEye es válida, pero no la preferimos como principal
    if fnorm.contains("battleye") || fname.to_lowercase().contains("_be") {
        s += 50;
    }
    s += depth * 3;
    s -= size_mb;
    s
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    // --- Helpers ---------------------------------------------------------------
    fn todos(conn: &Connection) -> Vec<library::GameCard> {
        library::list_games(conn, &library::LibraryFilter::default()).unwrap()
    }
    fn por_categoria(conn: &Connection, cat: i64) -> Vec<library::GameCard> {
        library::list_games(
            conn,
            &library::LibraryFilter {
                category_id: Some(cat),
                ..Default::default()
            },
        )
        .unwrap()
    }
    fn instalado(conn: &Connection) -> bool {
        todos(conn)[0].state == library::STATE_INSTALLED
    }
    /// `Discovered` mínimo para los tests que no pasan por el escáner.
    fn nuevo(key: &str, title: &str, exe: Option<&str>) -> library::Discovered {
        library::Discovered {
            source_key: key.into(),
            title: title.into(),
            exe_path: exe.map(String::from),
            platform: "PC".into(),
            ..Default::default()
        }
    }

    #[test]
    fn scans_folder_library_and_picks_right_exe() {
        let tmp = std::env::temp_dir().join(format!("sc_test_{}", std::process::id()));
        let game_dir = tmp.join("Cool Game");
        std::fs::create_dir_all(game_dir.join("redist")).unwrap();
        std::fs::write(game_dir.join("CoolGame.exe"), b"x").unwrap();
        std::fs::write(game_dir.join("cover.png"), b"x").unwrap(); // carátula local
        std::fs::write(game_dir.join("unins000.exe"), b"x").unwrap(); // excluido
        std::fs::write(game_dir.join("redist").join("vcredist_x64.exe"), b"x").unwrap(); // excluido

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let cat = library::create_category(&conn, "Coop Emu Games", None).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), Some(cat)).unwrap().0;

        let n = scan_all(&conn).unwrap();
        assert_eq!(n, 1, "debe detectar 1 juego");

        let games = por_categoria(&conn, cat);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Cool Game");

        let detail = library::get_game(&conn, games[0].id).unwrap().unwrap();
        assert!(detail.exe_path.as_deref().unwrap().ends_with("CoolGame.exe"), "elige el exe correcto, no unins/redist");
        assert!(detail.cover_path.as_deref().unwrap().ends_with("cover.png"), "detecta la carátula local");
        assert_eq!(detail.categories, vec!["Coop Emu Games".to_string()]);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn picks_deep_shipping_exe_not_diag_tool() {
        // Reproduce la estructura de Path of Titans (Unreal): el exe real está en
        // WindowsNoEditor\PathOfTitans\Binaries\Win64\ (nivel 5); no debe elegir el DiagReport.
        let tmp = std::env::temp_dir().join(format!("sc_test_pot_{}", std::process::id()));
        let game = tmp.join("Path of Titans");
        let win64 = game.join("WindowsNoEditor").join("PathOfTitans").join("Binaries").join("Win64");
        let diag = game.join("WindowsNoEditor").join("AlderonDiagReport");
        std::fs::create_dir_all(&win64).unwrap();
        std::fs::create_dir_all(&diag).unwrap();
        std::fs::write(diag.join("AlderonDiagReport-arm64.exe"), b"x").unwrap();
        std::fs::write(win64.join("PathOfTitans-Win64-Shipping.exe"), b"x").unwrap();
        std::fs::write(win64.join("PathOfTitans-Win64-Shipping_BE.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        assert_eq!(scan_all(&conn).unwrap(), 1);

        let games = todos(&conn);
        let detail = library::get_game(&conn, games[0].id).unwrap().unwrap();
        let exe = detail.exe_path.as_deref().unwrap();
        assert!(exe.ends_with("PathOfTitans-Win64-Shipping.exe"), "elige el shipping, no el DiagReport ni el _BE: {exe}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn scans_app_entry() {
        let tmp = std::env::temp_dir().join(format!("sc_test_app_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let exe = tmp.join("Dolphin.exe");
        std::fs::write(&exe, b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "app_entry", exe.to_str().unwrap(), None).unwrap().0;

        assert_eq!(scan_all(&conn).unwrap(), 1);
        let all = todos(&conn);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Dolphin");

        std::fs::remove_dir_all(&tmp).ok();
    }

    // ---- C.0_2 ----------------------------------------------------------------

    #[test]
    fn los_args_del_ejecutable_elegido_llegan_al_plan_de_lanzamiento() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) = library::upsert_game(&conn, 1, &nuevo("k1", "Juego", Some(r"C:\g\game.exe"))).unwrap();
        // Args del principal.
        library::update_game(
            &conn,
            id,
            &library::GameEdit {
                title: "Juego",
                platform: "PC",
                exe_path: Some(r"C:\g\game.exe"),
                exe_args: Some("-windowed -lang es"),
                ..Default::default()
            },
        )
        .unwrap();
        let plan = library::get_launch(&conn, id, None).unwrap().unwrap();
        assert_eq!(plan.args.as_deref(), Some("-windowed -lang es"));

        // Args de un ejecutable adicional (antes se descartaban).
        let eid = library::add_executable(
            &conn,
            id,
            "Dolphin + ROM",
            r"M:\EMUL4\Dolphin\Dolphin.exe",
            Some(r#"-e "M:\Mis Juegos\rom.iso""#),
        )
        .unwrap();
        let plan = library::get_launch(&conn, id, Some(eid)).unwrap().unwrap();
        assert_eq!(plan.exe.as_deref(), Some(r"M:\EMUL4\Dolphin\Dolphin.exe"));
        assert_eq!(plan.args.as_deref(), Some(r#"-e "M:\Mis Juegos\rom.iso""#));
        assert_eq!(plan.executable_id, Some(eid));
    }

    #[test]
    fn el_lanzamiento_queda_registrado() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) =
            library::upsert_game(&conn, 1, &nuevo("k1", "Juego", Some(r"C:\g\g.exe"))).unwrap();
        assert_eq!(library::get_game(&conn, id).unwrap().unwrap().launch_count, 0);

        library::log_launch(&conn, id, None, r"C:\g\g.exe").unwrap();
        library::log_launch(&conn, id, None, r"C:\g\g.exe").unwrap();
        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.launch_count, 2);
        assert!(d.last_launched.is_some(), "guarda la fecha del último lanzamiento");

        // Borrar el juego no deja registros huérfanos.
        library::delete_game(&conn, id).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM launch_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn juego_desinstalado_pasa_al_historico_y_se_recupera() {
        let tmp = std::env::temp_dir().join(format!("sc_test_missing_{}", std::process::id()));
        let game = tmp.join("Juego A");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("JuegoA.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let cat = library::create_category(&conn, "Mis juegos", None).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), Some(cat)).unwrap().0;
        scan_all(&conn).unwrap();
        assert!(instalado(&conn));

        // El usuario desinstala el juego (la carpeta-biblioteca sigue existiendo).
        std::fs::remove_dir_all(&game).unwrap();
        scan_all(&conn).unwrap();
        let g = todos(&conn);
        assert_eq!(g.len(), 1, "no se borra: conserva categorías y ediciones");
        assert_eq!(g[0].state, library::STATE_UNINSTALLED, "pasa al histórico");
        assert_eq!(
            library::get_game(&conn, g[0].id).unwrap().unwrap().categories,
            vec!["Mis juegos".to_string()]
        );

        // Y al reinstalarlo vuelve a estar disponible.
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("JuegoA.exe"), b"x").unwrap();
        scan_all(&conn).unwrap();
        assert!(instalado(&conn));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn fuente_inaccesible_no_marca_toda_la_biblioteca() {
        // Disco desconectado / carpeta borrada: NO debe marcar todo como desaparecido.
        let tmp = std::env::temp_dir().join(format!("sc_test_offline_{}", std::process::id()));
        let game = tmp.join("Juego B");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("JuegoB.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();

        std::fs::remove_dir_all(&tmp).unwrap(); // la raíz entera desaparece
        let s = &library::list_sources(&conn).unwrap()[0];
        assert!(
            discover(&s.kind, &s.path, &mut |_| true).is_none(),
            "ruta inaccesible se distingue de 0 resultados"
        );
        scan_all(&conn).unwrap();
        assert!(
            instalado(&conn),
            "la fuente se salta entera: los juegos no se marcan como perdidos"
        );
    }

    #[test]
    fn carpeta_sin_exe_reconocible_se_registra_para_poder_corregirla() {
        let tmp = std::env::temp_dir().join(format!("sc_test_noexe_{}", std::process::id()));
        let game = tmp.join("Juego Raro");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("setup.exe"), b"x").unwrap(); // único exe, y está excluido

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();

        let g = todos(&conn);
        assert_eq!(g.len(), 1, "no se descarta en silencio");
        assert!(g[0].needs_exe, "se marca para que el usuario asigne el exe");
        assert!(library::get_game(&conn, g[0].id).unwrap().unwrap().exe_path.is_none());

        std::fs::remove_dir_all(&tmp).ok();
    }

    // ---- Ronda 1: estados, exclusiones, filtros y popup de lanzamiento ---------

    /// La regla crítica de L.0_1: un excluido **no se resucita** al reescanear.
    #[test]
    fn un_excluido_nunca_vuelve_al_escanear() {
        let tmp = std::env::temp_dir().join(format!("sc_test_excl_{}", std::process::id()));
        let bueno = tmp.join("Juego Bueno");
        let basura = tmp.join("Saves");
        std::fs::create_dir_all(&bueno).unwrap();
        std::fs::create_dir_all(&basura).unwrap();
        std::fs::write(bueno.join("JuegoBueno.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        assert_eq!(todos(&conn).len(), 2, "la carpeta sin exe también entra");

        // El usuario excluye "Saves".
        let saves = todos(&conn)
            .into_iter()
            .find(|g| g.title == "Saves")
            .unwrap();
        library::set_state(&conn, saves.id, library::STATE_EXCLUDED).unwrap();

        // Por defecto ya no se ve...
        let visibles = todos(&conn);
        assert_eq!(visibles.len(), 1);
        assert_eq!(visibles[0].title, "Juego Bueno");

        // ...y tres reescaneos seguidos no lo devuelven a la vida.
        for _ in 0..3 {
            scan_all(&conn).unwrap();
        }
        assert_eq!(todos(&conn).len(), 1, "el excluido no reaparece");
        let excluidos = library::list_games(
            &conn,
            &library::LibraryFilter {
                state: Some(library::STATE_EXCLUDED.into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(excluidos.len(), 1, "sigue visible en la vista de Excluidos");

        // Y se puede desexcluir: vuelve en el siguiente escaneo.
        library::set_state(&conn, saves.id, library::STATE_INSTALLED).unwrap();
        scan_all(&conn).unwrap();
        assert_eq!(todos(&conn).len(), 2);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn un_excluido_no_se_marca_como_desinstalado_ni_cambia_de_titulo() {
        let tmp = std::env::temp_dir().join(format!("sc_test_excl2_{}", std::process::id()));
        let dir = tmp.join("Mods");
        std::fs::create_dir_all(&dir).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let id = todos(&conn)[0].id;
        library::set_state(&conn, id, library::STATE_EXCLUDED).unwrap();

        // Sigue en disco: el escaneo lo ve, lo cuenta como "skipped" y no lo toca.
        let s = &library::list_sources(&conn).unwrap()[0];
        let items = discover(&s.kind, &s.path, &mut |_| true).unwrap();
        let st = persist(&conn, s, &items).unwrap();
        assert_eq!(st.skipped, 1);
        assert_eq!(st.updated, 0);

        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.state, library::STATE_EXCLUDED);

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Marcar "no instalado" a mano es **reversible**, y el escaneo manda: si la carpeta
    /// sigue ahí, el juego vuelve a `installed` (guión L.0_2 §1).
    #[test]
    fn marcar_como_no_instalado_se_puede_deshacer() {
        let tmp = std::env::temp_dir().join(format!("sc_test_noinst_{}", std::process::id()));
        let juego = tmp.join("Juego Suelto");
        std::fs::create_dir_all(&juego).unwrap();
        std::fs::write(juego.join("JuegoSuelto.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let id = todos(&conn)[0].id;

        // A mano al histórico…
        library::set_state(&conn, id, library::STATE_UNINSTALLED).unwrap();
        assert_eq!(
            library::get_game(&conn, id).unwrap().unwrap().state,
            library::STATE_UNINSTALLED
        );
        // …y de vuelta, que es lo que antes no se podía hacer.
        library::set_state(&conn, id, library::STATE_INSTALLED).unwrap();
        assert!(instalado(&conn));

        // El escaneo también lo devuelve: la carpeta sigue en disco.
        library::set_state(&conn, id, library::STATE_UNINSTALLED).unwrap();
        scan_all(&conn).unwrap();
        assert!(instalado(&conn), "si está en disco, está instalado");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Ser favorito es **independiente** del estado: un juego del histórico puede serlo.
    #[test]
    fn favoritos_son_independientes_del_estado() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (a, _) = library::upsert_game(&conn, 1, &nuevo("ka", "A", Some("a.exe"))).unwrap();
        let (b, _) = library::upsert_game(&conn, 1, &nuevo("kb", "B", Some("b.exe"))).unwrap();

        library::set_favorite(&conn, a, true).unwrap();
        library::set_state(&conn, a, library::STATE_UNINSTALLED).unwrap();

        let favs = library::list_games(
            &conn,
            &library::LibraryFilter {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, a);
        assert!(favs[0].favorite);

        assert_eq!(library::state_counts(&conn, 14).unwrap().favorites, 1);
        // La card de detalle también lo ve: es lo que pinta la estrella del botón.
        assert!(library::get_game(&conn, a).unwrap().unwrap().favorite);
        assert!(!library::get_game(&conn, b).unwrap().unwrap().favorite);

        // Y se puede quitar.
        library::set_favorite(&conn, a, false).unwrap();
        assert_eq!(library::state_counts(&conn, 14).unwrap().favorites, 0);
        assert!(!library::get_game(&conn, a).unwrap().unwrap().favorite);
    }

    #[test]
    fn contadores_por_estado() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        for (i, st) in [
            library::STATE_INSTALLED,
            library::STATE_INSTALLED,
            library::STATE_UNINSTALLED,
            library::STATE_EXCLUDED,
        ]
        .iter()
        .enumerate()
        {
            let (id, _) = library::upsert_game(&conn, 1, &nuevo(&format!("k{i}"), &format!("J{i}"), Some("x.exe")))
                .unwrap();
            library::set_state(&conn, id, st).unwrap();
        }
        let c = library::state_counts(&conn, 14).unwrap();
        assert_eq!((c.installed, c.uninstalled, c.excluded), (2, 1, 0 + 1));
    }

    #[test]
    fn filtros_combinables() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let cat = library::create_category(&conn, "Coop", None).unwrap();

        let (a, _) = library::upsert_game(&conn, 1, &nuevo("ka", "Overcooked", Some("o.exe"))).unwrap();
        library::update_game(
            &conn,
            a,
            &library::GameEdit {
                title: "Overcooked",
                platform: "PC",
                exe_path: Some("o.exe"),
                players_min: Some(1),
                players_max: Some(4),
                ..Default::default()
            },
        )
        .unwrap();
        library::assign_category(&conn, a, cat).unwrap();

        let (b, _) =
            library::upsert_game(&conn, 1, &nuevo("kb", "Solo Game", Some("s.exe"))).unwrap();
        library::update_game(
            &conn,
            b,
            &library::GameEdit {
                title: "Solo Game",
                platform: "Switch",
                exe_path: Some("s.exe"),
                players_min: Some(1),
                players_max: Some(1),
                ..Default::default()
            },
        )
        .unwrap();

        // Sin exe → needs_exe.
        library::upsert_game(&conn, 1, &nuevo("kc", "Carpeta Rara", None)).unwrap();

        let f = |f: library::LibraryFilter| library::list_games(&conn, &f).unwrap();

        assert_eq!(f(library::LibraryFilter::default()).len(), 3);
        assert_eq!(
            f(library::LibraryFilter {
                players: Some(4),
                ..Default::default()
            })
            .len(),
            1,
            "solo el de 4 jugadores"
        );
        assert_eq!(
            f(library::LibraryFilter {
                platform: Some("Switch".into()),
                ..Default::default()
            })
            .len(),
            1
        );
        assert_eq!(
            f(library::LibraryFilter {
                needs_exe: Some(true),
                ..Default::default()
            })[0]
                .title,
            "Carpeta Rara"
        );
        // Combinado: categoría + texto + jugadores.
        assert_eq!(
            f(library::LibraryFilter {
                category_id: Some(cat),
                query: Some("over".into()),
                players: Some(2),
                ..Default::default()
            })
            .len(),
            1
        );
        // Combinado que no casa con nada.
        assert!(f(library::LibraryFilter {
            category_id: Some(cat),
            platform: Some("Switch".into()),
            ..Default::default()
        })
        .is_empty());
    }

    /// El fallo reportado: estado y categoría son **dimensiones independientes** y deben
    /// poder combinarse (los excluidos *de* una categoría).
    #[test]
    fn estado_y_categoria_se_combinan() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let steam = library::create_category(&conn, "Steam", None).unwrap();

        // Dos de la categoría (uno excluido) y uno excluido fuera de ella.
        let (a, _) = library::upsert_game(&conn, 1, &nuevo("ka", "A", Some("a.exe"))).unwrap();
        let (b, _) = library::upsert_game(&conn, 1, &nuevo("kb", "B", Some("b.exe"))).unwrap();
        let (c, _) = library::upsert_game(&conn, 1, &nuevo("kc", "C", Some("c.exe"))).unwrap();
        for id in [a, b] {
            library::assign_category(&conn, id, steam).unwrap();
        }
        library::set_state(&conn, b, library::STATE_EXCLUDED).unwrap();
        library::set_state(&conn, c, library::STATE_EXCLUDED).unwrap();

        let f = |f: library::LibraryFilter| library::list_games(&conn, &f).unwrap();

        // Solo categoría: el excluido no sale (regla de L.0_1).
        let solo_cat = f(library::LibraryFilter {
            category_id: Some(steam),
            ..Default::default()
        });
        assert_eq!(solo_cat.len(), 1);
        assert_eq!(solo_cat[0].title, "A");

        // Solo excluidos: los dos, de cualquier categoría.
        assert_eq!(
            f(library::LibraryFilter {
                state: Some(library::STATE_EXCLUDED.into()),
                ..Default::default()
            })
            .len(),
            2
        );

        // Combinado: los excluidos DE esa categoría.
        let combinado = f(library::LibraryFilter {
            state: Some(library::STATE_EXCLUDED.into()),
            category_id: Some(steam),
            ..Default::default()
        });
        assert_eq!(combinado.len(), 1);
        assert_eq!(combinado[0].title, "B");
    }

    #[test]
    fn filtros_de_origen_jugados_y_orden() {
        let tmp = std::env::temp_dir().join(format!("sc_test_orig_{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Juego Z")).unwrap();
        std::fs::write(tmp.join("Juego Z").join("JuegoZ.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let carpeta = todos(&conn)[0].id;

        // Una app añadida a mano, de otra fuente.
        let sid = library::add_source(&conn, "app_entry", r"C:\apps\Dolphin.exe", None).unwrap().0;
        let (app, _) =
            library::upsert_game(&conn, sid, &nuevo("kapp", "Dolphin", Some("d.exe"))).unwrap();

        let f = |f: library::LibraryFilter| library::list_games(&conn, &f).unwrap();

        // Origen.
        let solo_carpeta = f(library::LibraryFilter {
            origin: Some("folder_library".into()),
            ..Default::default()
        });
        assert_eq!(solo_carpeta.len(), 1);
        assert_eq!(solo_carpeta[0].id, carpeta);
        assert_eq!(
            f(library::LibraryFilter {
                origin: Some("app_entry".into()),
                ..Default::default()
            })[0]
                .id,
            app
        );

        // Jugados / sin jugar, horas totales, y orden por tiempo.
        let mapa = std::collections::HashMap::from([(
            "d.exe".to_string(),
            library::DatosRemotos {
                total_secs: 7200, // 2 h justas
                arte: None,
                app_id: 42,
            },
        )]);
        library::sync_timetrack(&conn, &mapa).unwrap();
        assert_eq!(
            f(library::LibraryFilter {
                played: Some(true),
                ..Default::default()
            })[0]
                .id,
            app
        );
        assert_eq!(
            f(library::LibraryFilter {
                played: Some(false),
                ..Default::default()
            })[0]
                .id,
            carpeta
        );
        assert_eq!(
            f(library::LibraryFilter {
                sort: Some("playtime".into()),
                ..Default::default()
            })[0]
                .id,
            app,
            "el más jugado va primero"
        );

        // Horas totales (guión T.0_2 §2). El juego lleva 2 h exactas.
        let banda = |min: Option<f64>, max: Option<f64>| {
            f(library::LibraryFilter {
                played: Some(true),
                min_hours: min,
                max_hours: max,
                ..Default::default()
            })
            .len()
        };
        assert_eq!(banda(Some(1.0), Some(5.0)), 1, "2 h cae en la banda de 1–5");
        assert_eq!(banda(None, Some(1.0)), 0, "2 h no es «menos de 1 h»");
        assert_eq!(banda(Some(5.0), None), 0, "2 h no es «más de 5 h»");
        // El máximo es **exclusivo**, y es lo que impide que dos bandas contiguas devuelvan dos
        // veces el juego que cae justo en la frontera.
        assert_eq!(banda(None, Some(2.0)), 0, "el máximo no incluye el valor exacto");
        assert_eq!(banda(Some(2.0), None), 1, "el mínimo sí lo incluye");

        // Y el id de TimeTrack llega al detalle: es lo que habilita el botón de gráficas.
        assert_eq!(library::get_game(&conn, app).unwrap().unwrap().tt_app_id, Some(42));

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Guión F.0_5: la dirección del orden va **aparte** del criterio, y la card trae lo que
    /// hace falta para partir la cuadrícula en secciones sin una consulta por juego.
    #[test]
    fn orden_con_direccion_y_datos_para_seccionar() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let sid = library::add_source(&conn, "app_entry", r"C:\apps", None).unwrap().0;
        let (a, _) = library::upsert_game(&conn, sid, &nuevo("ka", "Alfa", Some("a.exe"))).unwrap();
        let (z, _) = library::upsert_game(&conn, sid, &nuevo("kz", "Zulu", Some("z.exe"))).unwrap();

        let f = |x: library::LibraryFilter| library::list_games(&conn, &x).unwrap();
        let orden = |criterio: &str, desc: bool| -> Vec<i64> {
            f(library::LibraryFilter {
                sort: Some(criterio.into()),
                desc: Some(desc),
                ..Default::default()
            })
            .iter()
            .map(|c| c.id)
            .collect()
        };

        // Título en los dos sentidos: es lo que antes no se podía pedir.
        assert_eq!(orden("title", false), vec![a, z], "A → Z");
        assert_eq!(orden("title", true), vec![z, a], "Z → A");

        // Sin decir nada, manda la dirección natural: el título sube.
        assert_eq!(
            f(library::LibraryFilter::default())
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            vec![a, z]
        );

        // Los «sin dato» van al final **en las dos direcciones**: un juego que nunca se ha
        // lanzado no es el más antiguo, es que no hay fecha.
        library::log_launch(&conn, z, None, "z.exe").unwrap();
        assert_eq!(orden("recent", true)[0], z);
        assert_eq!(orden("recent", false)[0], z, "el que no tiene fecha, al final");

        // Y la card trae categorías y origen, que es lo que se usa para seccionar.
        let cat = library::create_category(&conn, "Coop", None).unwrap();
        library::assign_category(&conn, a, cat).unwrap();
        let cards = f(library::LibraryFilter::default());
        let card_a = cards.iter().find(|c| c.id == a).unwrap();
        let card_z = cards.iter().find(|c| c.id == z).unwrap();
        assert_eq!(card_a.categories, vec![cat]);
        assert!(card_z.categories.is_empty(), "sin categorías, lista vacía");
        assert_eq!(card_a.origin.as_deref(), Some("app_entry"));
    }

    #[test]
    fn el_popup_de_lanzamiento_solo_aparece_cuando_toca() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) = library::upsert_game(&conn, 1, &nuevo("k1", "Path of Titans", Some(r"C:\g\pot.exe")))
            .unwrap();

        // Sin launcher configurado → no se pregunta.
        let o = library::get_launch_options(&conn, id, None).unwrap().unwrap();
        assert!(!o.needs_prompt);

        // Con launcher → se pregunta.
        library::update_game(
            &conn,
            id,
            &library::GameEdit {
                title: "Path of Titans",
                platform: "PC",
                exe_path: Some(r"C:\g\pot.exe"),
                client_path: Some(r"C:\launcher\alderon.exe"),
                client_first: true,
                ..Default::default()
            },
        )
        .unwrap();
        let o = library::get_launch_options(&conn, id, None).unwrap().unwrap();
        assert!(o.needs_prompt);
        assert_eq!(o.client_path.as_deref(), Some(r"C:\launcher\alderon.exe"));

        // Al lanzar un ejecutable adicional concreto no se pregunta.
        let eid = library::add_executable(&conn, id, "BattlEye", r"C:\g\pot_be.exe", None).unwrap();
        assert!(
            !library::get_launch_options(&conn, id, Some(eid))
                .unwrap()
                .unwrap()
                .needs_prompt
        );

        // "No volver a preguntar" → deja de preguntar y recuerda la elección.
        library::remember_launch_mode(&conn, id, library::LaunchMode::GameOnly).unwrap();
        let o = library::get_launch_options(&conn, id, None).unwrap().unwrap();
        assert!(!o.needs_prompt);
        assert_eq!(o.remembered.as_deref(), Some("game"));

        // Y se puede volver a "preguntar siempre" desde la edición.
        library::remember_launch_mode(&conn, id, library::LaunchMode::Auto).unwrap();
        assert!(
            library::get_launch_options(&conn, id, None)
                .unwrap()
                .unwrap()
                .needs_prompt
        );
    }

    // ---- Ronda 5: carátulas ----------------------------------------------------

    #[test]
    fn solo_pide_caratula_quien_no_la_tiene() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        // Uno con carátula local del usuario, uno sin nada, y uno excluido.
        let mut con_cover = nuevo("k1", "Con carátula", Some("a.exe"));
        con_cover.cover_path = Some(r"C:\g\cover.png".into());
        library::upsert_game(&conn, 1, &con_cover).unwrap();
        let (pelado, _) =
            library::upsert_game(&conn, 1, &nuevo("k2", "Sin nada", Some("b.exe"))).unwrap();
        let (fuera, _) =
            library::upsert_game(&conn, 1, &nuevo("k3", "Excluido", Some("c.exe"))).unwrap();
        library::set_state(&conn, fuera, library::STATE_EXCLUDED).unwrap();

        let sin = library::list_sin_arte(&conn).unwrap();
        assert_eq!(sin.len(), 1, "ni el que ya tiene carátula ni el excluido");
        assert_eq!(sin[0].id, pelado);

        // Al resolverla, deja de pedirla; y la card la usa aunque no haya `cover_path`.
        library::set_art(&conn, pelado, r"C:\datos\icons\2.ico").unwrap();
        assert!(library::list_sin_arte(&conn).unwrap().is_empty());
        let card = todos(&conn).into_iter().find(|g| g.id == pelado).unwrap();
        assert_eq!(card.cover_path.as_deref(), Some(r"C:\datos\icons\2.ico"));

        // "Rehacer" olvida **todo lo automático**, y la imagen que encontró el escaneo en la
        // carpeta lo es: si no se limpiara, esos juegos no se volverían a mirar nunca, porque
        // `list_sin_arte` salta cualquiera que tenga `cover_path` (guión D.0_4 §3).
        let (con_id, _) = library::upsert_game(&conn, 1, &con_cover).unwrap();
        library::clear_art(&conn).unwrap();
        assert_eq!(
            library::list_sin_arte(&conn).unwrap().len(),
            2,
            "los dos vuelven a la cola: la del escaneo también era automática"
        );
        assert!(todos(&conn)
            .into_iter()
            .find(|g| g.id == con_id)
            .unwrap()
            .cover_path
            .is_none());
    }

    /// Las tres reglas que faltaban, y que entre las tres dejaban carátulas congeladas para
    /// siempre (guión D.0_4).
    #[test]
    fn la_caratula_puesta_a_mano_es_intocable_y_el_resto_no() {
        let tmp = std::env::temp_dir().join(format!("sc_test_d04_{}", std::process::id()));
        let dir = tmp.join("Juego Uno");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("JuegoUno.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let id = todos(&conn)[0].id;

        // 1. La elegida a mano sobrevive a un re-escaneo que encuentra imagen en la carpeta.
        library::set_cover(&conn, id, Some(r"D:\mias\portada.png")).unwrap();
        std::fs::write(dir.join("cover.png"), b"x").unwrap();
        scan_all(&conn).unwrap();
        assert_eq!(
            library::get_game(&conn, id).unwrap().unwrap().cover_path.as_deref(),
            Some(r"D:\mias\portada.png"),
            "un cover.png en la carpeta NO pisa la que eligió el usuario"
        );

        // 2. «Rehacer» tampoco la toca…
        library::clear_art(&conn).unwrap();
        assert!(library::list_sin_arte(&conn).unwrap().is_empty());

        // 3. …pero «Quitar la mía» suelta el seguro y vuelve a mandar la cascada.
        library::set_cover(&conn, id, None).unwrap();
        assert_eq!(library::list_sin_arte(&conn).unwrap().len(), 1);

        // 4. `get_game` ya no mezcla las dos: la automática va en su propio campo.
        library::set_art(&conn, id, r"C:\datos\icons\7.png").unwrap();
        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.cover_path, None, "no hay carátula propia");
        assert_eq!(d.art_path.as_deref(), Some(r"C:\datos\icons\7.png"));

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Lo que el usuario cura **sobrevive al reescaneo** (guión A.0_7).
    ///
    /// El caso que lo motivó: *Tom Clancy's Ghost Recon Wildlands* estaba marcado como **No
    /// instalado**, pero su carpeta sigue en Uplay —residual, sin ejecutable— y cada escaneo lo
    /// resucitaba como «instalado, sin ejecutable».
    #[test]
    fn el_reescaneo_no_pisa_lo_que_ha_curado_el_usuario() {
        let tmp = std::env::temp_dir().join(format!("sc_test_a07_{}", std::process::id()));
        let dir = tmp.join("Tom Clancys Ghost Recon Wildlands");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("captura.png"), b"x").unwrap(); // carpeta residual, sin .exe

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap();
        scan_all(&conn).unwrap();
        let id = todos(&conn)[0].id;

        // El usuario lo renombra y lo marca como no instalado.
        let d = library::get_game(&conn, id).unwrap().unwrap();
        library::update_game(
            &conn,
            id,
            &library::GameEdit {
                title: "Ghost Recon Wildlands",
                platform: &d.platform,
                exe_path: None,
                exe_args: None,
                client_path: None,
                client_args: None,
                client_first: false,
                players_min: None,
                players_max: None,
                launch_mode: None,
                steam_account: None,
            },
        )
        .unwrap();
        library::set_state(&conn, id, library::STATE_UNINSTALLED).unwrap();

        scan_all(&conn).unwrap();

        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.title, "Ghost Recon Wildlands", "el nombre que puso el usuario manda");
        assert_eq!(
            d.state,
            library::STATE_UNINSTALLED,
            "una carpeta residual sin ejecutable no vuelve a ser «instalado»"
        );
        assert_eq!(todos(&conn).len(), 1, "y no se ha duplicado la fila");

        // Pero si el juego se reinstala de verdad —aparece un ejecutable—, vuelve solo.
        std::fs::write(dir.join("GRW.exe"), b"x").unwrap();
        scan_all(&conn).unwrap();
        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.state, library::STATE_INSTALLED, "reinstalado: vuelve a estar");
        assert_eq!(d.title, "Ghost Recon Wildlands", "el nombre sigue siendo el suyo");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Guardar en el editor **sin tocar el nombre** no debe congelarlo: sería repetir el fallo
    /// de `D.0_4`, donde guardar ascendía la carátula automática a manual.
    #[test]
    fn guardar_sin_renombrar_no_congela_el_titulo() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) = library::upsert_game(&conn, 1, &nuevo("k1", "Nombre De Carpeta", Some("a.exe")))
            .unwrap();

        fn edit(t: &str) -> library::GameEdit<'_> {
            library::GameEdit {
                title: t,
                platform: "PC",
                exe_path: Some("a.exe"),
                exe_args: None,
                client_path: None,
                client_args: None,
                client_first: false,
                players_min: None,
                players_max: None,
                launch_mode: None,
                steam_account: None,
            }
        }
        library::update_game(&conn, id, &edit("Nombre De Carpeta")).unwrap();

        // El escaneo sigue pudiendo corregir el título, porque nadie lo renombró.
        library::upsert_game(&conn, 1, &nuevo("k1", "Nombre Corregido", Some("a.exe"))).unwrap();
        assert_eq!(
            library::get_game(&conn, id).unwrap().unwrap().title,
            "Nombre Corregido"
        );

        // En cuanto lo renombra de verdad, queda fijado.
        library::update_game(&conn, id, &edit("El Mío")).unwrap();
        library::upsert_game(&conn, 1, &nuevo("k1", "Otra Vez La Carpeta", Some("a.exe"))).unwrap();
        assert_eq!(library::get_game(&conn, id).unwrap().unwrap().title, "El Mío");
    }

    /// Un cambio de grafía en la ruta no debe hacer que el juego deje de reconocerse.
    #[test]
    fn el_juego_se_reconoce_aunque_cambie_la_caja_de_la_ruta() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) =
            library::upsert_game(&conn, 1, &nuevo(r"M:\Juegos\Uno", "Uno", Some("a.exe"))).unwrap();
        library::set_state(&conn, id, library::STATE_EXCLUDED).unwrap();

        // La misma carpeta, escrita de otra forma: tiene que ser **el mismo** juego.
        let (otro, que) =
            library::upsert_game(&conn, 1, &nuevo(r"m:/juegos/uno\", "Uno", Some("a.exe"))).unwrap();
        assert_eq!(otro, id);
        assert_eq!(que, library::Upsert::Skipped, "y sigue excluido");
        assert_eq!(todos(&conn).len(), 0, "no se ha creado una fila nueva");
    }

    /// Una categoría quitada a mano **no vuelve** en el siguiente escaneo (guión A.0_8).
    ///
    /// El caso real: una carpeta entera va a «Coop» por la regla de su fuente, pero dentro hay
    /// un juego que no lo es. Antes, quitárselo no servía de nada — y como se escanea al abrir
    /// el hub, la decisión no duraba ni una sesión.
    #[test]
    fn una_categoria_quitada_a_mano_no_vuelve_al_reescanear() {
        let tmp = std::env::temp_dir().join(format!("sc_test_a08_{}", std::process::id()));
        for n in ["Coop Uno", "Para Mí Solo"] {
            let d = tmp.join(n);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{}.exe", n.replace(' ', ""))), b"x").unwrap();
        }

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let cat = library::create_category(&conn, "Coop", None).unwrap();
        let sid = library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), Some(cat))
            .unwrap()
            .0;
        scan_all(&conn).unwrap();

        // La regla de la fuente se la pone a los dos.
        let solo = todos(&conn)
            .into_iter()
            .find(|g| g.title == "Para Mí Solo")
            .unwrap();
        assert!(library::get_game(&conn, solo.id).unwrap().unwrap().categories.contains(&"Coop".to_string()));

        // El usuario se la quita a uno…
        library::unassign_category(&conn, solo.id, cat).unwrap();
        scan_all(&conn).unwrap();
        assert!(
            !library::get_game(&conn, solo.id).unwrap().unwrap().categories.contains(&"Coop".to_string()),
            "la regla de la fuente no puede deshacer lo que el usuario decidió"
        );

        // …y al otro no le pasa nada: la regla sigue valiendo para los demás.
        let otro = todos(&conn).into_iter().find(|g| g.title == "Coop Uno").unwrap();
        assert!(library::get_game(&conn, otro.id).unwrap().unwrap().categories.contains(&"Coop".to_string()));

        // Y si el usuario se arrepiente y la vuelve a poner a mano, se levanta el rechazo.
        library::assign_category(&conn, solo.id, cat).unwrap();
        scan_all(&conn).unwrap();
        assert!(library::get_game(&conn, solo.id).unwrap().unwrap().categories.contains(&"Coop".to_string()));

        let _ = library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None);
        assert_eq!(sid, library::list_sources(&conn).unwrap()[0].id);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// El mismo rechazo (guión A.0_8) también vale contra el botón «Aplicar a todos» de
    /// Fuentes, no solo contra el reescaneo automático. Es la misma regla de fuente: no debe
    /// deshacer la decisión del usuario por ninguno de los dos caminos.
    #[test]
    fn una_categoria_quitada_a_mano_tampoco_vuelve_al_reafirmar_la_fuente() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        let cat = library::create_category(&conn, "Coop", None).unwrap();
        let sid = library::add_source(&conn, "folder_library", r"M:\Juegos", None)
            .unwrap()
            .0;
        let (solo, _) =
            library::upsert_game(&conn, sid, &nuevo("k1", "Para Mí Solo", Some("a.exe"))).unwrap();
        let (otro, _) =
            library::upsert_game(&conn, sid, &nuevo("k2", "Coop Uno", Some("a.exe"))).unwrap();

        library::assign_source_category(&conn, sid, cat, true).unwrap();
        assert!(library::get_game(&conn, solo).unwrap().unwrap().categories.contains(&"Coop".to_string()));

        // El usuario se la quita a uno…
        library::unassign_category(&conn, solo, cat).unwrap();

        // …y alguien vuelve a pulsar «Aplicar a todos» en Fuentes (p. ej. para que la
        // categoría le llegue a un juego nuevo de la misma carpeta).
        library::assign_source_category(&conn, sid, cat, true).unwrap();

        assert!(
            !library::get_game(&conn, solo).unwrap().unwrap().categories.contains(&"Coop".to_string()),
            "«Aplicar a todos» tampoco puede deshacer lo que el usuario decidió"
        );
        assert!(
            library::get_game(&conn, otro).unwrap().unwrap().categories.contains(&"Coop".to_string()),
            "y al que no la había rechazado, se la sigue poniendo"
        );
    }

    /// Añadir dos veces la misma carpeta no debe duplicar nada, aunque venga escrita distinta
    /// (guión A.0_6). En Windows `M:\Juegos`, `m:\juegos\` y `M:/Juegos` son la misma.
    #[test]
    fn una_carpeta_que_ya_es_fuente_no_se_anade_otra_vez() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        let cat = library::create_category(&conn, "Coop", None).unwrap();
        let (a, ya_a) = library::add_source(&conn, "folder_library", r"M:\Juegos", None).unwrap();
        assert!(!ya_a, "la primera vez se añade");

        for variante in [r"m:\juegos", r"M:\Juegos\", r"M:/Juegos", r"  M:\JUEGOS\  "] {
            let (id, ya) = library::add_source(&conn, "folder_library", variante, None).unwrap();
            assert!(ya, "«{variante}» es la misma carpeta");
            assert_eq!(id, a, "y devuelve la fuente que ya existía");
        }
        assert_eq!(library::list_sources(&conn).unwrap().len(), 1);

        // Lo único que aporta repetir la petición es la categoría: esa sí se aplica.
        let (id, ya) = library::add_source(&conn, "folder_library", r"m:\juegos", Some(cat)).unwrap();
        assert!(ya);
        assert_eq!(library::source_categories(&conn, id).unwrap(), vec![cat]);

        // La misma ruta con otro `kind` sí es otra fuente: no es la misma manera de leerla.
        let (_, ya) = library::add_source(&conn, "app_entry", r"M:\Juegos", None).unwrap();
        assert!(!ya);
        assert_eq!(library::list_sources(&conn).unwrap().len(), 2);
    }

    /// Las que ya estaban duplicadas se fusionan al abrir, **sin perder juegos** (A.0_6 §2).
    #[test]
    fn las_fuentes_ya_duplicadas_se_fusionan_al_abrir() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        // Se reproduce la base de datos **anterior**: hay que tirar el índice único para poder
        // insertar los duplicados, lo que de paso demuestra que ya no caben por ahí.
        conn.execute_batch("DROP INDEX idx_source_unica;").unwrap();
        for p in [r"M:\Juegos", r"m:\juegos\", r"M:/Juegos"] {
            conn.execute("INSERT INTO source (kind, path) VALUES ('folder_library', ?1)", [p])
                .unwrap();
        }
        let ids: Vec<i64> = library::list_sources(&conn).unwrap().iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), 3);

        // Un juego colgando de cada una, y una categoría de fuente en la tercera.
        for (n, sid) in ids.iter().enumerate() {
            library::upsert_game(&conn, *sid, &nuevo(&format!("k{n}"), &format!("J{n}"), Some("a.exe")))
                .unwrap();
        }
        let cat = library::create_category(&conn, "Coop", None).unwrap();
        library::assign_source_category(&conn, ids[2], cat, false).unwrap();

        let fusionadas = library::fusionar_fuentes_duplicadas(&conn).unwrap();
        assert_eq!(fusionadas, 2);

        let quedan = library::list_sources(&conn).unwrap();
        assert_eq!(quedan.len(), 1);
        assert_eq!(quedan[0].id, ids[0], "sobrevive la primera que se añadió");
        assert_eq!(quedan[0].game_count, 3, "los tres juegos pasan a colgar de ella");
        assert_eq!(todos(&conn).len(), 3, "y no se ha perdido ninguno");
        assert_eq!(
            library::source_categories(&conn, ids[0]).unwrap(),
            vec![cat],
            "la categoría de la duplicada se hereda"
        );
    }

    /// La reparación de lo que el editor ya ascendió a manual sin pedirlo (guión D.0_4 §1).
    #[test]
    fn se_reparan_las_caratulas_ascendidas_por_el_editor() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        // Lo que pasaba: abrir Editar y guardar escribía el icono del `.exe` en `cover_path`.
        let (id, _) = library::upsert_game(&conn, 1, &nuevo("k1", "Ascendido", Some("a.exe"))).unwrap();
        conn.execute(
            r"UPDATE game SET cover_path = 'C:\Users\L\AppData\Roaming\com.sofacervecero.hub\icons\1.png'
              WHERE id = ?1",
            [id],
        )
        .unwrap();
        // Y una de verdad, elegida por el usuario fuera de nuestra carpeta.
        let (mia, _) = library::upsert_game(&conn, 1, &nuevo("k2", "Mía", Some("b.exe"))).unwrap();
        library::set_cover(&conn, mia, Some(r"D:\mias\portada.png")).unwrap();

        library::init_schema(&conn).unwrap(); // la migración corre al abrir la BD

        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.cover_path, None, "deja de estar marcada como propia");
        assert!(
            d.art_path.as_deref().unwrap().ends_with(r"icons\1.png"),
            "pero la imagen no se pierde: vuelve a ser automática"
        );
        assert_eq!(
            library::get_game(&conn, mia).unwrap().unwrap().cover_path.as_deref(),
            Some(r"D:\mias\portada.png"),
            "la elegida de verdad no se toca"
        );
    }

    // ---- Ronda 5d: categorías por fuente y confirmación al lanzar --------------

    /// F10: *"todos los juegos que estén **o hayan estado** instalados en M:\\CoGames van a Coop"*.
    #[test]
    fn una_fuente_entera_se_asigna_a_una_categoria() {
        let tmp = std::env::temp_dir().join(format!("sc_test_f10_{}", std::process::id()));
        for n in ["Juego Uno", "Juego Dos"] {
            let d = tmp.join(n);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{}.exe", n.replace(' ', ""))), b"x").unwrap();
        }

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let sid = library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();

        // Uno se desinstala: debe recibir la categoría igualmente.
        std::fs::remove_dir_all(tmp.join("Juego Dos")).unwrap();
        scan_all(&conn).unwrap();

        let coop = library::create_category(&conn, "Coop", None).unwrap();
        let etiquetados = library::assign_source_category(&conn, sid, coop, true).unwrap();
        assert_eq!(etiquetados, 2, "también el que ya no está instalado");

        let en_coop = library::list_games(
            &conn,
            &library::LibraryFilter {
                category_id: Some(coop),
                state: Some(library::STATE_UNINSTALLED.into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(en_coop.len(), 1, "el del histórico también está en Coop");

        // Y lo que aparezca después hereda la categoría sin repetir la operación.
        std::fs::create_dir_all(tmp.join("Juego Tres")).unwrap();
        std::fs::write(tmp.join("Juego Tres").join("JuegoTres.exe"), b"x").unwrap();
        scan_all(&conn).unwrap();
        let tres = todos(&conn)
            .into_iter()
            .find(|g| g.title == "Juego Tres")
            .unwrap();
        assert_eq!(
            library::get_game(&conn, tres.id).unwrap().unwrap().categories,
            vec!["Coop".to_string()]
        );

        // Quitarla de la fuente puede limpiar también los juegos.
        library::unassign_source_category(&conn, sid, coop, true).unwrap();
        assert!(library::get_game(&conn, tres.id).unwrap().unwrap().categories.is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// C8: «Launcher SpaceWar» pregunta **siempre** antes de lanzar.
    #[test]
    fn una_categoria_puede_exigir_confirmacion_al_lanzar() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (id, _) =
            library::upsert_game(&conn, 1, &nuevo("k1", "Coop Steam", Some("g.exe"))).unwrap();

        // Sin categorías especiales no se pregunta.
        let o = library::get_launch_options(&conn, id, None).unwrap().unwrap();
        assert!(!o.needs_prompt);
        assert!(o.confirmaciones.is_empty());

        let sw = library::create_category(&conn, "Launcher SpaceWar", None).unwrap();
        library::set_category_prompt(&conn, sw, Some("¿Está abierta la cuenta de servicio?"))
            .unwrap();
        library::assign_category(&conn, id, sw).unwrap();

        let o = library::get_launch_options(&conn, id, None).unwrap().unwrap();
        assert!(o.needs_prompt, "la categoría obliga a preguntar");
        assert_eq!(o.confirmaciones.len(), 1);
        assert_eq!(o.confirmaciones[0].0, "Launcher SpaceWar");

        // Ni siquiera un modo recordado la salta: es una comprobación de seguridad.
        library::remember_launch_mode(&conn, id, library::LaunchMode::GameOnly).unwrap();
        assert!(
            library::get_launch_options(&conn, id, None)
                .unwrap()
                .unwrap()
                .needs_prompt
        );

        // Y al quitar el mensaje, deja de preguntar.
        library::set_category_prompt(&conn, sw, None).unwrap();
        assert!(
            !library::get_launch_options(&conn, id, None)
                .unwrap()
                .unwrap()
                .needs_prompt
        );
    }

    // ---- Ronda 2: CRUD de categorías y grupos ---------------------------------

    #[test]
    fn crud_del_arbol_de_categorias() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (gid, _) =
            library::upsert_game(&conn, 1, &nuevo("k1", "Juego", Some("j.exe"))).unwrap();

        let eje = library::create_category(&conn, "Nº Jugadores", None).unwrap();
        let coop = library::create_category(&conn, "Coop", Some(eje)).unwrap();
        let versus = library::create_category(&conn, "Versus", Some(eje)).unwrap();
        library::assign_category(&conn, gid, coop).unwrap();

        // Renombrar no pierde la asignación.
        library::rename_category(&conn, coop, "Coop local").unwrap();
        assert_eq!(
            library::get_game(&conn, gid).unwrap().unwrap().categories,
            vec!["Coop local".to_string()]
        );

        // Herencia (ADR-005 §4.1): el juego cuenta en la hija **y** en el eje.
        let cats = library::list_categories(&conn).unwrap();
        let busca = |n: &str| cats.iter().find(|c| c.name == n).unwrap();
        assert_eq!(busca("Coop local").count, 1);
        assert_eq!(busca("Nº Jugadores").count, 1, "el padre hereda a sus hijas");
        assert_eq!(busca("Versus").count, 0);

        // Y filtrar por el eje trae los de sus hijas.
        let por_eje = library::list_games(
            &conn,
            &library::LibraryFilter {
                category_id: Some(eje),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(por_eje.len(), 1, "filtrar por el eje trae los de la hija");

        // Orden manual (A3): el árbol sale padre → hijas, y `move` intercambia hermanas.
        let nombres: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(nombres, vec!["Nº Jugadores", "Coop local", "Versus"]);
        assert!(library::move_category(&conn, versus, -1).unwrap());
        let nombres: Vec<String> = library::list_categories(&conn)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(nombres, vec!["Nº Jugadores", "Versus", "Coop local"]);
        assert!(
            !library::move_category(&conn, versus, -1).unwrap(),
            "en el extremo no se mueve"
        );

        // No se puede crear un ciclo: el eje no puede colgar de su propia hija.
        assert!(library::set_category_parent(&conn, eje, Some(coop)).is_err());

        // Borrar una categoría no borra el juego, y sus hijas suben de nivel.
        library::delete_category(&conn, eje).unwrap();
        let cats = library::list_categories(&conn).unwrap();
        assert_eq!(cats.len(), 2, "las hijas sobreviven al borrar el padre");
        assert!(
            cats.iter().all(|c| c.parent_id.is_none()),
            "y suben a ser ejes"
        );
        assert_eq!(todos(&conn).len(), 1);
    }

    /// «Añadidos recientemente» (F.0_6). Lo delicado no es la ventana, es qué hacer con los
    /// juegos que ya estaban antes de que existiera la columna: de esos **no consta**.
    #[test]
    fn anadidos_recientemente_y_los_que_no_constan() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let (nuevo_id, _) =
            library::upsert_game(&conn, 1, &nuevo("kn", "Recién llegado", Some("n.exe"))).unwrap();
        let (viejo_id, _) =
            library::upsert_game(&conn, 1, &nuevo("kv", "De siempre", Some("v.exe"))).unwrap();

        // Se simula lo que deja la migración en lo que ya existía: sin fecha.
        conn.execute("UPDATE game SET added_at = NULL WHERE id = ?1", [viejo_id])
            .unwrap();

        let recientes = library::list_games(
            &conn,
            &library::LibraryFilter {
                added_days: Some(14),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(recientes.len(), 1, "solo el que tiene fecha de verdad");
        assert_eq!(recientes[0].id, nuevo_id);
        assert_eq!(library::state_counts(&conn, 14).unwrap().recent, 1);

        // Uno añadido hace un mes queda fuera de una ventana de 14 días, pero entra en 60.
        conn.execute(
            "UPDATE game SET added_at = datetime('now', '-30 days') WHERE id = ?1",
            [viejo_id],
        )
        .unwrap();
        assert_eq!(library::state_counts(&conn, 14).unwrap().recent, 1);
        assert_eq!(
            library::state_counts(&conn, 60).unwrap().recent,
            2,
            "la ventana es configurable"
        );

        // Y el orden «Añadido» usa la fecha real: primero el último en entrar.
        let orden: Vec<i64> = library::list_games(
            &conn,
            &library::LibraryFilter {
                sort: Some("added".into()),
                desc: Some(true),
                ..Default::default()
            },
        )
        .unwrap()
        .iter()
        .map(|c| c.id)
        .collect();
        assert_eq!(orden, vec![nuevo_id, viejo_id]);
    }

    /// Los «grupos» de texto de F.0_2 pasan a ser categorías padre de verdad (F.0_6).
    #[test]
    fn los_grupos_viejos_migran_a_categorias_padre() {
        let conn = Connection::open_in_memory().unwrap();
        // Esquema **anterior**, con `grp` y sin las columnas nuevas.
        conn.execute_batch(
            "CREATE TABLE category(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, grp TEXT);
             INSERT INTO category (name, grp) VALUES ('Steam', 'Clientes');
             INSERT INTO category (name, grp) VALUES ('EA', 'Clientes');
             INSERT INTO category (name, grp) VALUES ('Coop', 'TAGS');
             INSERT INTO category (name, grp) VALUES ('Suelta', NULL);",
        )
        .unwrap();
        library::init_schema(&conn).unwrap();

        let cats = library::list_categories(&conn).unwrap();
        let busca = |n: &str| cats.iter().find(|c| c.name == n).unwrap();
        let clientes = busca("Clientes");
        assert!(clientes.parent_id.is_none(), "el grupo pasa a ser un eje");
        assert_eq!(busca("Steam").parent_id, Some(clientes.id));
        assert_eq!(busca("EA").parent_id, Some(clientes.id));
        assert_eq!(busca("Coop").parent_id, Some(busca("TAGS").id));
        assert!(busca("Suelta").parent_id.is_none(), "sin grupo, se queda raíz");

        // Orden inicial alfabético entre hermanas (A2), que es el punto de partida de A3.
        assert!(busca("EA").sort_order < busca("Steam").sort_order);

        // Y es idempotente: reabrir no vuelve a migrar ni duplica nada.
        let antes = cats.len();
        library::init_schema(&conn).unwrap();
        assert_eq!(library::list_categories(&conn).unwrap().len(), antes);
    }

    /// Las categorías por defecto se añaden sin pisar lo que ya hay, y la recolocación
    /// se **propone** pero no se aplica sola (ADR-005 §8 #4 y #5).
    #[test]
    fn categorias_por_defecto_y_recolocacion_propuesta() {
        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();

        // El usuario ya tenía «Coop» colgando de TAGS, que es donde no le toca.
        let tags = library::create_category(&conn, "TAGS", None).unwrap();
        let coop = library::create_category(&conn, "Coop", Some(tags)).unwrap();

        let creadas = library::ensure_default_categories(&conn).unwrap();
        assert!(creadas.contains(&"Clientes".to_string()));
        assert!(creadas.contains(&"Couch Co-Op".to_string()));
        assert!(
            !creadas.contains(&"TAGS".to_string()),
            "lo que ya existía no se recrea"
        );

        // Idempotente: una segunda pasada no crea nada.
        assert!(library::ensure_default_categories(&conn).unwrap().is_empty());
        // Y no ha movido «Coop» de sitio: eso solo se propone.
        let cats = library::list_categories(&conn).unwrap();
        assert_eq!(
            cats.iter().find(|c| c.name == "Coop").unwrap().parent_id,
            Some(tags)
        );

        let sug = library::sugerir_recolocacion(&conn).unwrap();
        let s = sug.iter().find(|s| s.name == "Coop").expect("se propone mover Coop");
        assert_eq!(s.desde.as_deref(), Some("TAGS"));
        assert_eq!(s.hacia, "Nº Jugadores");

        // Solo se aplica lo que se confirma.
        assert_eq!(library::aplicar_recolocacion(&conn, &[coop]).unwrap(), 1);
        let cats = library::list_categories(&conn).unwrap();
        let jugadores = cats.iter().find(|c| c.name == "Nº Jugadores").unwrap();
        assert_eq!(
            cats.iter().find(|c| c.name == "Coop").unwrap().parent_id,
            Some(jugadores.id)
        );
        // Ya colocada, deja de proponerse.
        assert!(library::sugerir_recolocacion(&conn)
            .unwrap()
            .iter()
            .all(|s| s.name != "Coop"));
    }

    #[test]
    fn eliminar_una_fuente_con_y_sin_sus_juegos() {
        let tmp = std::env::temp_dir().join(format!("sc_test_src_{}", std::process::id()));
        let game = tmp.join("Juego C");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("JuegoC.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let sid =
            library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let gid = todos(&conn)[0].id;
        library::log_launch(&conn, gid, None, "x").unwrap();
        assert_eq!(library::list_sources(&conn).unwrap()[0].game_count, 1);

        // Sin borrar juegos: la entrada sobrevive, huérfana de fuente.
        assert_eq!(library::remove_source(&conn, sid, false).unwrap(), 0);
        assert!(library::list_sources(&conn).unwrap().is_empty());
        assert_eq!(todos(&conn).len(), 1);

        // Con borrado: se lleva juego, categorías, ejecutables y registro.
        let sid =
            library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        assert_eq!(library::remove_source(&conn, sid, true).unwrap(), 1);
        assert!(todos(&conn).is_empty());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM launch_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "sin registros huérfanos");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn fuente_desactivada_no_se_escanea() {
        let tmp = std::env::temp_dir().join(format!("sc_test_off_{}", std::process::id()));
        let game = tmp.join("Juego D");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("JuegoD.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        let sid =
            library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        library::set_source_enabled(&conn, sid, false).unwrap();
        assert_eq!(scan_all(&conn).unwrap(), 0);
        assert!(todos(&conn).is_empty());

        library::set_source_enabled(&conn, sid, true).unwrap();
        assert_eq!(scan_all(&conn).unwrap(), 1);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn migrates_pre_c_schema() {
        let conn = Connection::open_in_memory().unwrap();
        // Esquema antiguo (antes de guión C): sin source_key/client_*/exe_locked.
        conn.execute_batch(
            "CREATE TABLE game(
                id INTEGER PRIMARY KEY, source_id INTEGER, title TEXT NOT NULL,
                install_dir TEXT, exe_path TEXT, platform TEXT NOT NULL DEFAULT 'PC', cover_path TEXT
            );
            INSERT INTO game(id, title, install_dir, exe_path, platform)
            VALUES (1, 'Old Game', 'C:\\g', 'C:\\g\\old.exe', 'PC');",
        )
        .unwrap();

        // init_schema debe migrar sin perder datos.
        library::init_schema(&conn).unwrap();
        let d = library::get_game(&conn, 1).unwrap().unwrap();
        assert_eq!(d.title, "Old Game");
        assert!(!d.client_first);
        let key: String = conn
            .query_row("SELECT source_key FROM game WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(!key.is_empty(), "source_key backfilled");
    }

    #[test]
    fn manual_exe_edit_survives_rescan() {
        let tmp = std::env::temp_dir().join(format!("sc_test_lock_{}", std::process::id()));
        let game = tmp.join("MyGame");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("MyGame.exe"), b"x").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        library::init_schema(&conn).unwrap();
        library::add_source(&conn, "folder_library", tmp.to_str().unwrap(), None).unwrap().0;
        scan_all(&conn).unwrap();
        let id = todos(&conn)[0].id;

        // El usuario corrige el exe a mano (queda "locked").
        library::update_game(
            &conn,
            id,
            &library::GameEdit {
                title: "MyGame",
                platform: "PC",
                exe_path: Some(r"C:\custom\real.exe"),
                ..Default::default()
            },
        )
        .unwrap();
        // Un re-escaneo NO debe pisar el exe curado.
        scan_all(&conn).unwrap();
        let d = library::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(d.exe_path.as_deref(), Some(r"C:\custom\real.exe"));

        std::fs::remove_dir_all(&tmp).ok();
    }
}
