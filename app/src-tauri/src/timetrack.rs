//! Integración con la app **TimeTrack** (NO tracker propio): consumo de su API local para
//! el tiempo jugado y para el arte de las cards. Cerrar el hub **nunca** mata TimeTrack.
//! Ver plan §4.5 y `ARCHITECTURE/02-integracion-timetrack.md` (ADR-003).

use serde::Serialize;
use std::time::Duration;

/// Dónde escucha TimeTrack por defecto (FastAPI + uvicorn).
pub const BASE_POR_DEFECTO: &str = "http://127.0.0.1:31337";

/// Estado de TimeTrack respecto a este hub.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Estado {
    /// No responde en el puerto: no está arrancado (o no está instalado).
    NoResponde,
    /// Responde: la API está viva.
    Conectado { profile_id: i64, profile: String },
}

/// Una app vista por TimeTrack, ya normalizada para correlacionar con nuestra biblioteca.
#[derive(Debug, Clone)]
pub struct AppRemota {
    /// Id de la app **en TimeTrack**. Hace falta para pedirle sus gráficas: toda su API por app
    /// —`/stats`, `/weekly`, `/monthly`, `/sessions`— va por id, no por `exe_name` (guión T.0_2).
    pub app_id: i64,
    /// Clave de correlación: nombre del ejecutable en minúsculas.
    pub exe_name: String,
    pub total_secs: i64,
    /// Rutas absolutas del arte, ya resueltas contra el directorio de TimeTrack.
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub poster: Option<String>,
}

fn get(url: &str) -> Option<serde_json::Value> {
    // Timeouts cortos: si TimeTrack no está, no queremos bloquear el hub.
    let resp = ureq::builder()
        .timeout_connect(Duration::from_millis(600))
        .timeout_read(Duration::from_secs(4))
        .build()
        .get(url)
        .call()
        .ok()?;
    serde_json::from_str(&resp.into_string().ok()?).ok()
}

/// ¿Está TimeTrack vivo? Devuelve también el perfil activo, que es el que consultaremos.
pub fn estado(base: &str) -> Estado {
    if get(&format!("{base}/api/tracker/status")).is_none() {
        return Estado::NoResponde;
    }
    let perfiles = get(&format!("{base}/api/profiles")).unwrap_or(serde_json::Value::Null);
    let activo = perfiles
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|p| p["is_active"].as_i64().unwrap_or(0) != 0)
                .or_else(|| a.first())
        })
        .cloned();
    match activo {
        Some(p) => Estado::Conectado {
            profile_id: p["id"].as_i64().unwrap_or(1),
            profile: p["name"].as_str().unwrap_or("Default").to_string(),
        },
        None => Estado::NoResponde,
    }
}

/// Normaliza un `exe_path` nuestro a la clave que usa TimeTrack (`tracked_apps.exe_name`).
pub fn exe_name(exe_path: &str) -> String {
    std::path::Path::new(exe_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_path)
        .to_lowercase()
}

/// Resuelve la ruta en disco de una imagen de TimeTrack.
/// El valor guardado puede ser un nombre de fichero o una ruta; nos quedamos con el nombre y
/// lo buscamos en `<dir>/images/app-images/`, que es donde las deja su `image_fetcher`.
fn ruta_arte(dir: Option<&str>, valor: &serde_json::Value) -> Option<String> {
    let dir = dir?;
    let v = valor.as_str()?.trim();
    if v.is_empty() || v.starts_with("http://") || v.starts_with("https://") {
        return None;
    }
    let nombre = v.rsplit(['/', '\\']).next()?;
    let p = std::path::Path::new(dir)
        .join("images")
        .join("app-images")
        .join(nombre);
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

/// Lee las apps del perfil + sus totales de tiempo. `dir` es la carpeta de TimeTrack, para
/// resolver el arte en disco (así no hay que descargar ni duplicar imágenes).
pub fn apps_del_perfil(base: &str, profile_id: i64, dir: Option<&str>) -> Vec<AppRemota> {
    let apps = match get(&format!("{base}/api/profiles/{profile_id}/apps")) {
        Some(v) => v,
        None => return Vec::new(),
    };
    // `summary` da el total por app, y hay que pedirle **todo** el histórico.
    //
    // No vale `days=N` alto: TimeTrack lo valida a **365 como máximo** (por encima contesta 422).
    // Es decir, `days=365` —lo que había— es el techo de la API, y el día en que el histórico pase
    // del año los totales se truncarían **en silencio**: el tiempo mostrado bajaría sin motivo
    // aparente y el filtro por horas (guión T.0_2 §2) empezaría a mentir.
    //
    // Lo que sí llega a todo es el rango explícito, que es lo que usa su propio botón «All»:
    // `/api/stats/date-range` da el primer y último día con datos, y se pasan como `from`/`to`.
    // Comprobado contra la instalación real: `days=365` y el rango completo dan exactamente lo
    // mismo hoy (74 apps, 583,5 h), así que el cambio no altera nada salvo el techo.
    let rango = get(&format!("{base}/api/stats/date-range"))
        .and_then(|r| {
            let d = r["first_date"].as_str()?.to_string();
            let h = r["last_date"].as_str()?.to_string();
            Some(format!("from={d}&to={h}"))
        })
        // Si no se puede saber el rango, el techo de la API es mejor que nada.
        .unwrap_or_else(|| "days=365".to_string());
    let resumen = get(&format!(
        "{base}/api/profiles/{profile_id}/summary?{rango}"
    ))
    .unwrap_or(serde_json::Value::Null);

    let mut totales: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    if let Some(arr) = resumen.as_array() {
        for it in arr {
            if let Some(exe) = it["exe_name"].as_str() {
                let secs = it["total_secs"]
                    .as_i64()
                    .or_else(|| it["total_seconds"].as_i64())
                    .unwrap_or(0);
                totales.insert(exe.to_lowercase(), secs);
            }
        }
    }

    apps.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let exe = a["exe_name"].as_str()?.to_lowercase();
                    Some(AppRemota {
                        app_id: a["id"].as_i64()?,
                        total_secs: totales.get(&exe).copied().unwrap_or(0),
                        icon: ruta_arte(dir, &a["img_icon"]),
                        banner: ruta_arte(dir, &a["img_banner"]),
                        poster: ruta_arte(dir, &a["img_poster"]),
                        exe_name: exe,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_el_exe_a_la_clave_de_timetrack() {
        assert_eq!(exe_name(r"M:\LGames\PoT\Binaries\Win64\PathOfTitans.exe"), "pathoftitans.exe");
        assert_eq!(exe_name("Dolphin.EXE"), "dolphin.exe");
        assert_eq!(exe_name("/mnt/juegos/Game.exe"), "game.exe");
    }

    /// Comprobación **contra el TimeTrack real** de la máquina. Ignorado por defecto: los
    /// tests no deben depender de que el servicio esté arrancado.
    /// Ejecutar con: `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn integracion_con_timetrack_vivo() {
        let base = BASE_POR_DEFECTO;
        match estado(base) {
            Estado::NoResponde => panic!("TimeTrack no responde en {base}; arráncalo para este test"),
            Estado::Conectado { profile_id, profile } => {
                println!("perfil activo: {profile} (id {profile_id})");
                let apps = apps_del_perfil(base, profile_id, None);
                assert!(!apps.is_empty(), "debería devolver apps del perfil");
                assert!(
                    apps.iter().all(|a| a.app_id > 0),
                    "toda app trae su id de TimeTrack: es la clave de sus gráficas"
                );
                let con_tiempo = apps.iter().filter(|a| a.total_secs > 0).count();
                let horas: f64 = apps.iter().map(|a| a.total_secs as f64).sum::<f64>() / 3600.0;
                println!(
                    "{} apps, {} con tiempo registrado, {horas:.1} h en total",
                    apps.len(),
                    con_tiempo
                );
                assert!(con_tiempo > 0, "alguna app debe tener tiempo acumulado");
                assert!(
                    apps.iter().all(|a| a.exe_name == a.exe_name.to_lowercase()),
                    "las claves de correlación llegan en minúsculas"
                );
            }
        }
    }

    #[test]
    fn el_arte_remoto_o_vacio_se_ignora() {
        assert!(ruta_arte(None, &serde_json::json!("x.png")).is_none());
        assert!(ruta_arte(Some("C:/tt"), &serde_json::json!("")).is_none());
        assert!(ruta_arte(Some("C:/tt"), &serde_json::json!("https://x/y.png")).is_none());
        assert!(ruta_arte(Some("C:/tt"), &serde_json::json!(null)).is_none());
        // Un fichero que no existe tampoco se devuelve: la card no debe apuntar a la nada.
        assert!(ruta_arte(Some("C:/tt"), &serde_json::json!("no-existe.png")).is_none());
    }
}
