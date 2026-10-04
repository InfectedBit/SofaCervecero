//! Proveedor de **Steam** (apartado B / M3). Detecta la instalación, recorre sus bibliotecas
//! y lee lo instalado desde los ficheros que el propio cliente mantiene:
//! `steamapps/libraryfolders.vdf` (dónde están las bibliotecas) y `appmanifest_<appid>.acf`
//! (qué hay instalado en cada una). Ver plan §4.2 y guión B.0_1.
//!
//! No se usa ninguna librería de VDF: el formato es de pares `"clave" "valor"` y solo
//! necesitamos unas pocas claves, así que un extractor de ~15 líneas basta y no añade peso.

use std::path::{Path, PathBuf};

/// Un juego instalado según Steam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JuegoSteam {
    pub appid: String,
    pub nombre: String,
    /// Carpeta real del juego: `<biblioteca>/steamapps/common/<installdir>`.
    pub install_dir: PathBuf,
}

/// Extrae el valor de `"clave"   "valor"` en un texto VDF. Devuelve la **primera** aparición.
pub fn valor_manifest(texto: &str, clave: &str) -> Option<String> {
    valor_vdf(texto, clave)
}

fn valor_vdf(texto: &str, clave: &str) -> Option<String> {
    let patron = format!("\"{clave}\"");
    for linea in texto.lines() {
        let l = linea.trim();
        // La clave debe estar entrecomillada entera, para que "name" no case con "LastOwner".
        let Some(resto) = l.strip_prefix(&patron) else {
            continue;
        };
        let resto = resto.trim_start();
        if !resto.starts_with('"') {
            continue;
        }
        let valor: String = resto[1..].chars().take_while(|c| *c != '"').collect();
        return Some(valor.replace("\\\\", "\\"));
    }
    None
}

/// Todas las apariciones de `"path"` en `libraryfolders.vdf`.
fn rutas_vdf(texto: &str) -> Vec<PathBuf> {
    texto
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let resto = l.strip_prefix("\"path\"")?.trim_start();
            let v: String = resto.strip_prefix('"')?.chars().take_while(|c| *c != '"').collect();
            Some(PathBuf::from(v.replace("\\\\", "\\")))
        })
        .collect()
}

/// Localiza la carpeta de Steam. Registro primero; si no, las rutas habituales.
pub fn carpeta_steam() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(k) = hkcu.open_subkey(r"Software\Valve\Steam") {
            if let Ok(p) = k.get_value::<String, _>("SteamPath") {
                let p = PathBuf::from(p.replace('/', "\\"));
                if p.is_dir() {
                    return Some(p);
                }
            }
        }
    }
    [
        r"C:\Program Files (x86)\Steam",
        r"C:\Program Files\Steam",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_dir())
}

/// Cuenta con la que Steam iniciará sesión (`AutoLoginUser`). Es la forma barata de saber
/// "qué cuenta está activa" sin tocar la sesión del usuario (plan §4.3).
pub fn cuenta_activa() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let k = hkcu.open_subkey(r"Software\Valve\Steam").ok()?;
        let u: String = k.get_value("AutoLoginUser").ok()?;
        return (!u.trim().is_empty()).then(|| u.to_lowercase());
    }
    #[cfg(not(windows))]
    None
}

/// Clave de comparación de rutas en Windows: sin distinguir mayúsculas ni separador final.
/// El registro devuelve `c:/program files/steam` y el VDF `C:\Program Files\STEAM`: es la
/// **misma** carpeta, y sin normalizar se escanearía dos veces.
fn clave_ruta(p: &Path) -> String {
    p.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Bibliotecas de Steam (la principal + las extra de `libraryfolders.vdf`), sin repetidos.
pub fn bibliotecas(steam: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam.to_path_buf()];
    let mut vistas: std::collections::HashSet<String> =
        std::iter::once(clave_ruta(steam)).collect();
    let vdf = steam.join("steamapps").join("libraryfolders.vdf");
    if let Ok(txt) = std::fs::read_to_string(&vdf) {
        for p in rutas_vdf(&txt) {
            if p.is_dir() && vistas.insert(clave_ruta(&p)) {
                out.push(p);
            }
        }
    }
    out.retain(|p| p.join("steamapps").is_dir());
    out
}

/// Lee los `appmanifest_*.acf` de una biblioteca y devuelve lo que sigue instalado en disco.
pub fn juegos_de(biblioteca: &Path) -> Vec<JuegoSteam> {
    let dir = biblioteca.join("steamapps");
    let Ok(entradas) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entradas.flatten() {
        let p = e.path();
        let nombre_fichero = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !nombre_fichero.starts_with("appmanifest_") || !nombre_fichero.ends_with(".acf") {
            continue;
        }
        let Ok(txt) = std::fs::read_to_string(&p) else {
            continue;
        };
        let (Some(appid), Some(nombre), Some(installdir)) = (
            valor_vdf(&txt, "appid"),
            valor_vdf(&txt, "name"),
            valor_vdf(&txt, "installdir"),
        ) else {
            continue;
        };
        // Steam conserva manifiestos de cosas ya borradas: solo cuenta lo que existe.
        let carpeta = dir.join("common").join(&installdir);
        if !carpeta.is_dir() {
            continue;
        }
        // Steamworks Common Redistributables y similares no son juegos.
        if appid == "228980" {
            continue;
        }
        out.push(JuegoSteam {
            appid,
            nombre,
            install_dir: carpeta,
        });
    }
    out.sort_by(|a, b| a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase()));
    out
}

/// Todo lo instalado, de todas las bibliotecas.
pub fn escanear(steam: &Path) -> Vec<JuegoSteam> {
    bibliotecas(steam).iter().flat_map(|b| juegos_de(b)).collect()
}

/// URI de lanzamiento vía cliente (plan §4.3): más robusto que el `.exe` directo, porque
/// deja que Steam resuelva DRM, actualizaciones y parámetros del juego.
pub fn uri_lanzamiento(appid: &str) -> String {
    format!("steam://rungameid/{appid}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
"AppState"
{
	"appid"		"1375530"
	"Universe"		"1"
	"LauncherPath"		"D:\\Steam\\steam.exe"
	"name"		"Exo One: Prologue"
	"installdir"		"Exo One Prologue"
	"LastOwner"		"76561198090175842"
}
"#;

    #[test]
    fn lee_las_claves_del_appmanifest() {
        assert_eq!(valor_vdf(MANIFEST, "appid").as_deref(), Some("1375530"));
        assert_eq!(valor_vdf(MANIFEST, "name").as_deref(), Some("Exo One: Prologue"));
        assert_eq!(
            valor_vdf(MANIFEST, "installdir").as_deref(),
            Some("Exo One Prologue")
        );
        // Las barras dobles del VDF se desescapan.
        assert_eq!(
            valor_vdf(MANIFEST, "LauncherPath").as_deref(),
            Some(r"D:\Steam\steam.exe")
        );
        assert!(valor_vdf(MANIFEST, "no_existe").is_none());
    }

    #[test]
    fn la_clave_debe_coincidir_entera() {
        // "name" no debe casar con "LastOwner" ni con claves que lo contengan.
        let vdf = "\t\"nameplate\"\t\t\"malo\"\n\t\"name\"\t\t\"bueno\"\n";
        assert_eq!(valor_vdf(vdf, "name").as_deref(), Some("bueno"));
    }

    #[test]
    fn extrae_todas_las_bibliotecas() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files\\STEAM"
	}
	"1"
	{
		"path"		"D:\\SteamLibraryD"
	}
}
"#;
        let rutas = rutas_vdf(vdf);
        assert_eq!(rutas.len(), 2);
        assert_eq!(rutas[0], PathBuf::from(r"C:\Program Files\STEAM"));
        assert_eq!(rutas[1], PathBuf::from(r"D:\SteamLibraryD"));
    }

    #[test]
    fn las_rutas_se_comparan_sin_distinguir_mayusculas_ni_separador() {
        // Caso real de esta máquina: el registro da minúsculas con `/`, el VDF mayúsculas con `\`.
        assert_eq!(
            clave_ruta(Path::new("c:/program files/steam")),
            clave_ruta(Path::new(r"C:\Program Files\STEAM"))
        );
        assert_eq!(
            clave_ruta(Path::new(r"D:\SteamLibraryD\")),
            clave_ruta(Path::new(r"d:\steamlibraryd"))
        );
        assert_ne!(
            clave_ruta(Path::new(r"D:\SteamLibraryD")),
            clave_ruta(Path::new(r"G:\SteamLibrary"))
        );
    }

    #[test]
    fn uri_de_lanzamiento() {
        assert_eq!(uri_lanzamiento("42690"), "steam://rungameid/42690");
    }

    /// Contra la instalación real de la máquina. Ignorado por defecto.
    /// `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn integracion_con_steam_instalado() {
        let steam = carpeta_steam().expect("no se encontró la carpeta de Steam");
        println!("Steam en: {}", steam.display());
        let libs = bibliotecas(&steam);
        println!("bibliotecas: {libs:?}");
        assert!(!libs.is_empty());
        let juegos = escanear(&steam);
        println!("juegos instalados: {}", juegos.len());
        for j in juegos.iter().take(8) {
            println!("  [{}] {} → {}", j.appid, j.nombre, j.install_dir.display());
        }
        assert!(!juegos.is_empty(), "debería encontrar algún juego instalado");
        println!("cuenta activa: {:?}", cuenta_activa());
    }
}
