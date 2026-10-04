//! Orquestador de lanzamiento (Procedimientos) + colocación en monitor.
//! Ver plan §4.3 / §4.4. M1: exe directo (store/emulador/tool en fases posteriores).
#![allow(dead_code)]

use std::io::{Error, ErrorKind};
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Procedimiento de lanzamiento de un juego. El lanzamiento NO es "ejecutar el .exe" sin más.
#[derive(Debug, Clone)]
pub enum LaunchMethod {
    /// Ejecutable directo (cwd = carpeta del juego).
    Exe { path: String, args: Vec<String> },
    /// Vía cliente de tienda; p.ej. `steam://rungameid/<id>` con cuenta requerida.
    Store { uri: String, required_account: Option<String> },
    /// Emulador + ROM (Dolphin, shadPS4…).
    Emulator { emulator: String, rom: String, args: Vec<String> },
    /// Herramienta externa (Nucleus Coop y similares para splitscreen).
    Tool { tool: String, args: Vec<String> },
}

/// Divide una línea de argumentos respetando **comillas dobles**, como hace Windows.
///
/// `split_whitespace()` partía `--rom "M:\Mis Juegos\rom.iso"` en tres argumentos y rompía
/// cualquier ruta con espacios (C.0_2 §1). Reglas:
/// - las comillas agrupan y no se incluyen en el argumento;
/// - `\"` produce una comilla literal;
/// - `""` produce un argumento vacío (se conserva, puede ser significativo).
///
/// No re-escapa nada: cada token se pasa tal cual a `Command::arg`, que ya se encarga del
/// entrecomillado al construir la línea de comandos real de Windows.
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
                has_token = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// Abre una URI de protocolo (`steam://rungameid/<appid>`, `com.epicgames.launcher://`…).
///
/// Se delega en el shell de Windows, que es quien sabe qué cliente la registra. Se usa
/// `cmd /C start` con `CREATE_NO_WINDOW` para que no parpadee una consola: el primer
/// argumento vacío de `start` es el título de ventana, obligatorio para que no confunda la
/// URI con un título.
pub fn abrir_uri(uri: &str) -> std::io::Result<()> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", uri]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()?;
    Ok(())
}

/// Decide **qué** abrir en el explorador a partir de las rutas registradas de un juego.
///
/// Se separa de `abrir_en_explorador` para poder probar la decisión sin arrancar ventanas.
/// Devuelve la ruta y si hay que *seleccionar* el fichero dentro de su carpeta; el orden es:
///
/// 1. el ejecutable, si existe → se abre la carpeta **con él seleccionado**;
/// 2. la carpeta de instalación, si existe;
/// 3. la carpeta del ejecutable, si existe (el `.exe` se ha movido o borrado, pero la carpeta
///    sigue ahí: es lo más útil que se puede ofrecer);
/// 4. nada, con un error que nombra la ruta que falla — «no se abre» sin más no se depura.
pub fn ruta_a_abrir(
    install_dir: Option<&str>,
    exe_path: Option<&str>,
) -> std::io::Result<(String, bool)> {
    if let Some(exe) = exe_path.filter(|p| Path::new(p).is_file()) {
        return Ok((exe.to_string(), true));
    }
    if let Some(dir) = install_dir.filter(|p| Path::new(p).is_dir()) {
        return Ok((dir.to_string(), false));
    }
    if let Some(padre) = exe_path
        .map(Path::new)
        .and_then(Path::parent)
        .filter(|p| p.is_dir())
    {
        return Ok((padre.to_string_lossy().into_owned(), false));
    }
    let culpable = install_dir.or(exe_path);
    Err(Error::new(
        ErrorKind::NotFound,
        match culpable {
            Some(p) => format!("la ruta ya no existe: {p}"),
            None => String::from("este juego no tiene carpeta ni ejecutable registrados"),
        },
    ))
}

/// Abre una ventana del explorador en la carpeta de un juego (guión A.0_5 §2).
///
/// Se usa `raw_arg` a propósito: `explorer` **no** parsea su línea de comandos como el resto de
/// los programas y rechaza `"/select,C:\Mis Juegos\x.exe"` entrecomillado entero, que es
/// justo lo que produciría `Command::arg`. La forma que entiende es `/select,"ruta"`.
pub fn abrir_en_explorador(
    install_dir: Option<&str>,
    exe_path: Option<&str>,
) -> std::io::Result<()> {
    let (ruta, seleccionar) = ruta_a_abrir(install_dir, exe_path)?;
    // Una barra final se comería la comilla de cierre: `"C:\dir\"` deja de ser una cadena.
    let limpia = ruta.trim_end_matches(['\\', '/']);
    let mut cmd = Command::new("explorer");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if seleccionar {
            cmd.raw_arg(format!("/select,\"{limpia}\""));
        } else {
            cmd.raw_arg(format!("\"{limpia}\""));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = seleccionar;
        cmd.arg(limpia);
    }
    // `explorer` devuelve código 1 incluso cuando abre la ventana, así que no se espera a ver
    // cómo acaba: si el `spawn` funciona, está hecho.
    cmd.spawn()?;
    Ok(())
}

/// Lanza un ejecutable con args opcionales y el cwd correcto (por defecto, su carpeta).
/// Valida antes que el fichero exista para poder dar un error útil en la UI.
pub fn launch_exe(exe: &str, args: Option<&str>, cwd: Option<&str>) -> std::io::Result<Child> {
    if !Path::new(exe).is_file() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("no existe el ejecutable: {exe}"),
        ));
    }
    let mut cmd = Command::new(exe);
    if let Some(a) = args {
        cmd.args(split_args(a));
    }
    match cwd {
        Some(dir) if Path::new(dir).is_dir() => {
            cmd.current_dir(dir);
        }
        _ => {
            if let Some(parent) = Path::new(exe).parent() {
                cmd.current_dir(parent);
            }
        }
    }
    cmd.spawn()
}

/// Lanza el cliente/launcher previo (Steam, Epic, launcher propio…) y espera **lo justo**.
///
/// Sustituye al `sleep(1500ms)` ciego: si el cliente ya estaba abierto, el proceso que
/// arrancamos delega en la instancia viva y termina enseguida → seguimos de inmediato. Si
/// arranca en frío, esperamos hasta `max_wait`.
///
/// *Limitación conocida:* "proceso vivo" no equivale a "cliente listo"; la comprobación real
/// de disponibilidad (p.ej. `steam://` respondiendo) llega con la integración de tiendas (M3).
pub fn launch_client_and_wait(
    exe: &str,
    args: Option<&str>,
    max_wait: Duration,
) -> std::io::Result<()> {
    let mut child = launch_exe(exe, args, None)?;
    let start = Instant::now();
    while start.elapsed() < max_wait {
        if child.try_wait()?.is_some() {
            return Ok(()); // ya estaba abierto: no hay que esperar más
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn respeta_comillas_en_rutas_con_espacios() {
        assert_eq!(
            split_args(r#"--rom "M:\Mis Juegos\rom.iso" -f"#),
            vec![r"--rom", r"M:\Mis Juegos\rom.iso", "-f"]
        );
    }

    #[test]
    fn caso_simple_y_vacios() {
        assert_eq!(split_args("-a -b"), vec!["-a", "-b"]);
        assert_eq!(split_args("   "), Vec::<String>::new());
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  a   b  "), vec!["a", "b"]);
    }

    #[test]
    fn comilla_escapada_y_argumento_vacio() {
        assert_eq!(split_args(r#"--title \"GOTY\""#), vec!["--title", "\"GOTY\""]);
        assert_eq!(split_args(r#"--name "" -x"#), vec!["--name", "", "-x"]);
    }

    #[test]
    fn comillas_pegadas_al_valor() {
        // Estilo habitual en emuladores: clave="valor con espacios"
        assert_eq!(
            split_args(r#"--config="C:\Program Files\emu\cfg.ini""#),
            vec![r"--config=C:\Program Files\emu\cfg.ini"]
        );
    }

    /// Las cuatro ramas de `ruta_a_abrir`, sobre ficheros de verdad: la decisión depende del
    /// disco, así que probarla con rutas inventadas solo comprobaría el último caso.
    #[test]
    fn que_abrir_segun_lo_que_exista() {
        use super::ruta_a_abrir;

        let base = std::env::temp_dir().join("sofacervecero-rutas");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let exe = base.join("juego.exe");
        std::fs::write(&exe, b"").unwrap();
        let (dir_s, exe_s) = (base.to_string_lossy(), exe.to_string_lossy());

        // 1. Con el ejecutable en su sitio, se selecciona.
        assert_eq!(
            ruta_a_abrir(Some(&dir_s), Some(&exe_s)).unwrap(),
            (exe_s.to_string(), true)
        );
        // 2. Sin ejecutable registrado, la carpeta.
        assert_eq!(
            ruta_a_abrir(Some(&dir_s), None).unwrap(),
            (dir_s.to_string(), false)
        );
        // 3. El `.exe` ya no está pero su carpeta sí: se abre la carpeta, sin seleccionar.
        std::fs::remove_file(&exe).unwrap();
        assert_eq!(
            ruta_a_abrir(None, Some(&exe_s)).unwrap(),
            (dir_s.to_string(), false)
        );
        // 4. Nada que abrir: el error nombra la ruta, que es lo que se puede depurar.
        std::fs::remove_dir_all(&base).unwrap();
        let err = ruta_a_abrir(None, Some(&exe_s)).unwrap_err().to_string();
        assert!(err.contains("juego.exe"), "{err}");
        assert!(ruta_a_abrir(None, None).is_err());
    }
}
