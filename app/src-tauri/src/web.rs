//! Cliente HTTP mínimo para las fuentes de arte remotas (guión D.0_3).
//!
//! **Por qué existe este módulo y no se llama desde el WebView.** Se probaron las cinco
//! fuentes con una cabecera `Origin` de la app, y esto es lo que contestan:
//!
//! | Fuente | Alcanzable | `Access-Control-Allow-Origin` |
//! | --- | --- | --- |
//! | Steam store search | sí | **ninguna** |
//! | Steam appdetails | sí | **ninguna** |
//! | SteamGridDB | sí | **ninguna** (tampoco en el *preflight* de `Authorization`) |
//! | GOG | sí | solo `https://www.gog.com` |
//! | RAWG | sí | `*` |
//!
//! O sea: **solo RAWG se podría llamar desde el WebView**. Y aunque todas lo permitieran, la
//! descarga tendría que pasar igualmente por aquí, porque el arte se **guarda en disco**
//! (`art_path`) y el WebView no escribe ficheros. El CDN de Valve es la excepción que confunde:
//! ahí la UI solo apunta un `<img src>`, que no pasa por CORS y no guarda nada.
//!
//! El TLS lo pone `native-tls`, que en Windows es **schannel**: sin almacén de raíces propio y
//! sin compilar `ring`. Ver ADR-003.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Identificarse es de buena educación con APIs públicas y gratuitas, y permite que nos
/// bloqueen a nosotros y no a «un cliente desconocido» si algo va mal.
const AGENTE: &str = "SofaCervecero/0.1 (+hub de juegos local)";

/// Tope de descarga. Una carátula son 100–400 KB; 12 MB es holgado y evita que una respuesta
/// rara —o un redirect a algo que no es una imagen— se coma la memoria.
const MAX_BYTES: u64 = 12 * 1024 * 1024;

fn agente() -> Option<&'static ureq::Agent> {
    static AG: OnceLock<Option<ureq::Agent>> = OnceLock::new();
    AG.get_or_init(|| {
        // Si schannel no arranca (caso rarísimo), se degrada a «no hay fuentes remotas» en vez
        // de reventar: el resto de la cascada es local y sigue funcionando.
        let tls = native_tls::TlsConnector::new().ok()?;
        Some(
            ureq::AgentBuilder::new()
                .tls_connector(std::sync::Arc::new(tls))
                .timeout_connect(Duration::from_secs(8))
                .timeout(Duration::from_secs(15))
                .user_agent(AGENTE)
                .build(),
        )
    })
    .as_ref()
}

/// GET que devuelve el cuerpo como texto. `autorizacion` es el valor literal de la cabecera
/// `Authorization` (SteamGridDB pide `Bearer <key>`).
pub fn texto(url: &str, autorizacion: Option<&str>) -> Result<String, String> {
    let ag = agente().ok_or("no se pudo iniciar TLS")?;
    let mut req = ag.get(url);
    if let Some(a) = autorizacion {
        req = req.set("Authorization", a);
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    resp.into_string().map_err(|e| e.to_string())
}

/// GET que solo comprueba que el recurso existe. El CDN de Valve devuelve 404 para los
/// tamaños de carátula que un juego concreto no tiene, así que hay que preguntar antes.
pub fn existe(url: &str) -> bool {
    agente()
        .map(|ag| ag.head(url).call().is_ok())
        .unwrap_or(false)
}

/// Descarga a `destino`, **conservando la extensión real** que diga la URL.
///
/// Devuelve la ruta escrita, que puede no ser la de entrada: igual que `art::icono_de_exe`,
/// quien llama tiene que quedarse con la devuelta.
pub fn descargar(url: &str, destino_sin_ext: &Path) -> Result<PathBuf, String> {
    let ag = agente().ok_or("no se pudo iniciar TLS")?;
    let resp = ag.get(url).call().map_err(|e| e.to_string())?;

    // La extensión sale de la URL (sin la query), y si no se reconoce, del `Content-Type`.
    let ext = extension(url, resp.header("Content-Type").unwrap_or(""));
    let destino = destino_sin_ext.with_extension(ext);

    let mut datos = Vec::new();
    resp.into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut datos)
        .map_err(|e| e.to_string())?;
    if datos.len() < 512 {
        // Un «png» de 40 bytes es una página de error, no una carátula.
        return Err(format!("respuesta demasiado pequeña ({} B)", datos.len()));
    }
    if let Some(padre) = destino.parent() {
        std::fs::create_dir_all(padre).ok();
    }
    std::fs::write(&destino, &datos).map_err(|e| e.to_string())?;
    Ok(destino)
}

/// Extensión de imagen a partir de la URL, con el `Content-Type` como red de seguridad.
fn extension(url: &str, content_type: &str) -> String {
    let limpia = url.split(['?', '#']).next().unwrap_or(url);
    let por_url = Path::new(limpia)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    if let Some(e) = por_url.filter(|e| matches!(e.as_str(), "jpg" | "jpeg" | "png" | "webp")) {
        return e;
    }
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/png" => "png".into(),
        "image/webp" => "webp".into(),
        _ => "jpg".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::extension;

    #[test]
    fn la_extension_sale_de_la_url_y_si_no_del_tipo() {
        // SteamGridDB sirve `.../1234.png?t=abc`: la query no es parte del nombre.
        assert_eq!(extension("https://x/1234.png?t=abc", ""), "png");
        assert_eq!(extension("https://x/a.JPG", ""), "jpg");
        assert_eq!(extension("https://x/a.webp", ""), "webp");
        // RAWG sirve rutas sin extensión reconocible: manda la cabecera.
        assert_eq!(extension("https://x/media/crop/abc", "image/png"), "png");
        // Y si no hay nada de lo que tirar, jpg: es lo que devuelven todas estas APIs.
        assert_eq!(extension("https://x/media/abc", ""), "jpg");
        assert_eq!(extension("https://x/a.svg", "image/jpeg"), "jpg");
    }
}
