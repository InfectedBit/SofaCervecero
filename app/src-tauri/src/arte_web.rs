//! Fuentes de arte **remotas**, al estilo de las de TimeTrack (guión D.0_3).
//!
//! Las locales (`art.rs`) resuelven el 100 % de la biblioteca, pero 136 de 197 juegos acaban
//! enseñando **el icono del `.exe` estirado a 150×225**, que es por lo que «algunos no se ven
//! bien». Todos ellos vienen de carpetas sueltas, sin `appid`, así que el CDN de Valve —que la
//! UI ya usa— no llega a dispararse nunca. Lo que falta es **buscar por nombre**.
//!
//! Tres fuentes, en este orden:
//!
//! | Fuente | Clave | Qué devuelve | Cubre |
//! | --- | --- | --- | --- |
//! | Steam (búsqueda por nombre → CDN) | no | póster vertical 600×900 | catálogo de Steam |
//! | SteamGridDB | sí, gratis | póster vertical 600×900 | **todo**: no-Steam, emulados, mods |
//! | RAWG | sí, gratis | captura **apaisada** | 500 000+ fichas |
//!
//! **GOG se queda fuera a propósito.** TimeTrack la tiene, pero `api.gog.com/products?search=`
//! devuelve hoy `[]` para cualquier término —probado con *Cyberpunk 2077*, que es suyo— y el
//! `embed.gog.com/games/ajax/filtered` tampoco responde ya. Implementarla sería implementar
//! una función que no encuentra nada.

use std::path::{Path, PathBuf};

use crate::web;

/// Qué fuentes remotas están activas y con qué claves. Sale de los ajustes del core.
#[derive(Default, Clone)]
pub struct Fuentes {
    /// Buscar en la tienda de Steam por nombre. No necesita clave.
    pub steam: bool,
    /// Clave de SteamGridDB, si la hay y está activada.
    pub sgdb: Option<String>,
    /// Clave de RAWG, si la hay y está activada.
    pub rawg: Option<String>,
}

impl Fuentes {
    /// ¿Hay algo remoto que intentar? Si no, ni se toca la red.
    pub fn alguna(&self) -> bool {
        self.steam || self.sgdb.is_some() || self.rawg.is_some()
    }
}

/// De dónde salió una carátula, para el resumen de la tarea.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Origen {
    SteamBuscado,
    SteamGridDb,
    Rawg,
}

/// Normaliza un título para compararlo: minúsculas y solo letras y números.
///
/// `"Risk of Rain 2"` y `"risk-of-rain-2"` son el mismo juego; `"Subnautica"` y
/// `"Subnautica 2"` **no**, y esa es justo la que hay que no equivocar.
fn normalizar(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Elige entre los candidatos el que **se llama igual** que lo buscado.
///
/// Devuelve `None` si ninguno coincide exactamente, y eso es deliberado: buscar *Subnautica*
/// en la tienda de Steam devuelve **«Subnautica 2» en primer lugar**, así que quedarse con el
/// primer resultado pondría la carátula equivocada. Una carátula equivocada es peor que
/// ninguna: la de verdad se nota que falta, la falsa no.
fn elegir<T, N: Fn(&T) -> String>(buscado: &str, candidatos: &[T], nombre: N) -> Option<usize> {
    let objetivo = normalizar(buscado);
    if objetivo.is_empty() {
        return None;
    }
    candidatos
        .iter()
        .position(|c| normalizar(&nombre(c)) == objetivo)
}

/// Quita de un título lo que no ayuda a buscarlo: ediciones, años de relanzamiento y ruido
/// de carpeta. `"Dark Souls III - Deluxe Edition"` → `"Dark Souls III"`.
fn titulo_buscable(t: &str) -> String {
    let bajo = t.to_lowercase();
    let mut corte = t.len();
    for marca in [
        " - deluxe", " deluxe edition", " - goty", " goty edition", " game of the year",
        " definitive edition", " enhanced edition", " complete edition", " remastered",
        " - ultimate", " ultimate edition", " [", " (",
    ] {
        if let Some(i) = bajo.find(marca) {
            corte = corte.min(i);
        }
    }
    t[..corte].trim().to_string()
}

// ── Steam: búsqueda por nombre → appid → CDN ─────────────────────────────────

/// Busca un juego por nombre en la tienda de Steam y devuelve su `appid`.
///
/// Es la fuente que más rinde porque **no necesita clave** y porque el CDN de Valve ya sirve
/// el póster vertical exacto que pinta la tarjeta.
pub fn appid_por_nombre(titulo: &str) -> Option<String> {
    let busca = titulo_buscable(titulo);
    let url = format!(
        "https://store.steampowered.com/api/storesearch/?term={}&l=spanish&cc=ES",
        urlencode(&busca)
    );
    let cuerpo = web::texto(&url, None).ok()?;
    let v: serde_json::Value = serde_json::from_str(&cuerpo).ok()?;
    let items = v.get("items")?.as_array()?;
    let i = elegir(&busca, items, |it| {
        it.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string()
    })?;
    items[i].get("id").and_then(|x| x.as_u64()).map(|n| n.to_string())
}

/// URL del póster vertical del CDN de Valve, **comprobando que existe**: no todos los appid
/// tienen `library_600x900` y el CDN contesta 404 para los que no.
pub fn poster_steam(appid: &str) -> Option<String> {
    let url = format!(
        "https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_600x900.jpg"
    );
    web::existe(&url).then_some(url)
}

// ── SteamGridDB ──────────────────────────────────────────────────────────────

/// Póster vertical de SteamGridDB. Es la fuente que cubre lo que Steam no tiene: juegos de
/// otras tiendas, emulados, mods y recopilaciones.
pub fn poster_sgdb(titulo: &str, key: &str) -> Option<String> {
    let busca = titulo_buscable(titulo);
    let auth = format!("Bearer {key}");
    let buscado = web::texto(
        &format!(
            "https://www.steamgriddb.com/api/v2/search/autocomplete/{}",
            urlencode(&busca)
        ),
        Some(&auth),
    )
    .ok()?;
    let v: serde_json::Value = serde_json::from_str(&buscado).ok()?;
    let datos = v.get("data")?.as_array()?;
    // Aquí sí se acepta el primer resultado si no hay coincidencia exacta: el buscador de
    // SteamGridDB es por juego, no por catálogo de tienda, y no mete secuelas por delante.
    let i = elegir(&busca, datos, |d| {
        d.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string()
    })
    .unwrap_or(0);
    let id = datos.get(i)?.get("id")?.as_u64()?;

    // `dimensions=600x900` es justo la proporción de la tarjeta; `types=static` descarta los
    // animados, que pesan y no se mueven en un `<img>`.
    let grids = web::texto(
        &format!(
            "https://www.steamgriddb.com/api/v2/grids/game/{id}?dimensions=600x900&types=static&nsfw=false&humor=false"
        ),
        Some(&auth),
    )
    .ok()?;
    let g: serde_json::Value = serde_json::from_str(&grids).ok()?;
    g.get("data")?
        .as_array()?
        .first()?
        .get("url")?
        .as_str()
        .map(String::from)
}

// ── RAWG ─────────────────────────────────────────────────────────────────────

/// Imagen de RAWG. **Es apaisada** (una captura, no un póster), así que va la última y viene
/// desactivada de fábrica: en una tarjeta vertical se recorta mal.
pub fn imagen_rawg(titulo: &str, key: &str) -> Option<String> {
    let busca = titulo_buscable(titulo);
    let url = format!(
        "https://api.rawg.io/api/games?search={}&key={}&page_size=5",
        urlencode(&busca),
        urlencode(key)
    );
    let cuerpo = web::texto(&url, None).ok()?;
    let v: serde_json::Value = serde_json::from_str(&cuerpo).ok()?;
    let res = v.get("results")?.as_array()?;
    let i = elegir(&busca, res, |r| {
        r.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string()
    })?;
    res[i]
        .get("background_image")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

// ── Orquestador ──────────────────────────────────────────────────────────────

/// Intenta las fuentes remotas en orden y **guarda** la primera que acierte.
///
/// Devuelve la ruta escrita y de dónde salió. `destino_sin_ext` marca la ruta base: la
/// extensión real la decide la descarga, así que hay que quedarse con la ruta devuelta.
pub fn buscar_poster(
    titulo: &str,
    appid_conocido: Option<&str>,
    f: &Fuentes,
    destino_sin_ext: &Path,
) -> Option<(PathBuf, Origen)> {
    if f.steam {
        // Si ya se sabe el appid (escaneo de Steam o deducido de la carpeta), no hace falta
        // buscar por nombre: se va directo al CDN y nos ahorramos una petición y un fallo.
        let appid = appid_conocido
            .map(String::from)
            .or_else(|| appid_por_nombre(titulo));
        if let Some(url) = appid.as_deref().and_then(poster_steam) {
            if let Ok(p) = web::descargar(&url, destino_sin_ext) {
                return Some((p, Origen::SteamBuscado));
            }
        }
    }
    if let Some(key) = f.sgdb.as_deref() {
        if let Some(url) = poster_sgdb(titulo, key) {
            if let Ok(p) = web::descargar(&url, destino_sin_ext) {
                return Some((p, Origen::SteamGridDb));
            }
        }
    }
    if let Some(key) = f.rawg.as_deref() {
        if let Some(url) = imagen_rawg(titulo, key) {
            if let Ok(p) = web::descargar(&url, destino_sin_ext) {
                return Some((p, Origen::Rawg));
            }
        }
    }
    None
}

/// Codifica un término para meterlo en una URL. Se hace a mano para no añadir una dependencia
/// por tres líneas: todo lo que no sea alfanumérico o `-_.~` va en `%XX`.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{elegir, normalizar, titulo_buscable, urlencode};

    #[test]
    fn normalizar_ignora_puntuacion_pero_no_los_numeros() {
        assert_eq!(normalizar("Risk of Rain 2"), normalizar("risk-of-rain-2"));
        assert_eq!(normalizar("S.T.A.L.K.E.R."), "stalker");
        // El número es parte del nombre: una secuela no es el mismo juego.
        assert_ne!(normalizar("Subnautica"), normalizar("Subnautica 2"));
    }

    /// El caso real que motivó la regla: la tienda de Steam devuelve «Subnautica 2» **primero**
    /// al buscar «Subnautica». Quedarse con el primer resultado pondría la carátula de la
    /// secuela, y una carátula equivocada no se nota — es peor que no tener ninguna.
    #[test]
    fn no_se_acepta_una_secuela_como_coincidencia() {
        let items = ["Subnautica 2", "Subnautica: Below Zero", "Subnautica"];
        assert_eq!(elegir("Subnautica", &items, |s| s.to_string()), Some(2));
        assert_eq!(
            elegir("Subnautica", &["Subnautica 2", "Subnautica: Below Zero"], |s| s.to_string()),
            None
        );
        assert_eq!(elegir("", &items, |s| s.to_string()), None);
    }

    #[test]
    fn se_recortan_las_ediciones_del_titulo() {
        assert_eq!(titulo_buscable("Dark Souls III - Deluxe Edition"), "Dark Souls III");
        assert_eq!(titulo_buscable("Skyrim (2011)"), "Skyrim");
        assert_eq!(titulo_buscable("DOOM [RePack]"), "DOOM");
        assert_eq!(titulo_buscable("Green Hell"), "Green Hell");
    }

    #[test]
    fn urlencode_escapa_espacios_y_acentos() {
        assert_eq!(urlencode("Green Hell"), "Green%20Hell");
        assert_eq!(urlencode("Ori & the Blind Forest"), "Ori%20%26%20the%20Blind%20Forest");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }
}
