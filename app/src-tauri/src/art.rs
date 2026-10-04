//! Carátulas y arte **sin APIs de terceros** (guiones D.0_1 y D.0_2).
//!
//! La cascada, de más barato a más caro:
//! 1. imagen ya presente en la carpeta del juego — la resuelve el escáner (`sources.rs`);
//! 2. **arte propio del usuario en Steam** (`userdata\…\config\grid`) y la **caché del
//!    cliente** (`appcache\librarycache`): ya están en disco, sin red;
//! 3. **icono del propio `.exe`**, leído de los recursos del PE — sin red y sin GDI;
//! 4. CDN de Valve — sin clave, pero con internet; lo pinta la UI directamente.
//!
//! Los saltos 2 y 3 viven aquí. Ninguno necesita clave, cuenta ni librería externa.

use std::path::{Path, PathBuf};

// ── 2. Caché local de Steam ──────────────────────────────────────────────────

/// Busca el póster vertical que Steam ya tiene descargado para un `appid`.
///
/// Se prueban las dos disposiciones que ha usado el cliente: una carpeta por appid, y los
/// ficheros sueltos con el appid como prefijo (versiones recientes).
pub fn caratula_steam_local(steam: &Path, appid: &str) -> Option<PathBuf> {
    // 1) Arte que el **usuario** haya puesto a mano en Steam: manda sobre la oficial.
    if let Some(p) = grid_de_usuario(steam, appid) {
        return Some(p);
    }
    // 2) La que el cliente ya descargó. Por orden: póster vertical → banner → fondo.
    let cache = steam.join("appcache").join("librarycache");
    const NOMBRES: &[&str] = &["library_600x900.jpg", "header.jpg", "library_hero.jpg"];
    for n in NOMBRES {
        let p = cache.join(appid).join(n);
        if p.is_file() {
            return Some(p);
        }
        let plano = cache.join(format!("{appid}_{n}"));
        if plano.is_file() {
            return Some(plano);
        }
    }
    None
}

/// Carátulas personalizadas del usuario: `userdata\<cuenta>\config\grid\<appid>p.<ext>`.
/// Es lo que se ve al arrastrar una imagen sobre un juego en la biblioteca de Steam.
fn grid_de_usuario(steam: &Path, appid: &str) -> Option<PathBuf> {
    let cuentas = std::fs::read_dir(steam.join("userdata")).ok()?;
    for cuenta in cuentas.flatten() {
        let grid = cuenta.path().join("config").join("grid");
        if !grid.is_dir() {
            continue;
        }
        // El sufijo `p` es el póster vertical; sin sufijo, el horizontal.
        for nombre in [format!("{appid}p"), appid.to_string()] {
            for ext in ["png", "jpg", "jpeg", "webp"] {
                let p = grid.join(format!("{nombre}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Deduce el **appid** de un juego a partir de su carpeta de instalación.
///
/// Hace falta porque una biblioteca de Steam se puede haber añadido como carpeta normal
/// (`G:\SteamLibrary\steamapps\common`), y entonces el juego no trae appid: sin él no se
/// puede mirar en la caché de Steam. Se busca el `steamapps` que cuelga por encima y se
/// cruza el nombre de la carpeta con el `installdir` de cada `appmanifest_*.acf`.
pub fn appid_desde_carpeta(install_dir: &Path) -> Option<String> {
    let carpeta = install_dir.file_name()?.to_str()?;
    // `…\steamapps\common\<juego>` → subir hasta `steamapps`.
    let comun = install_dir.parent()?;
    if !comun.file_name()?.eq_ignore_ascii_case("common") {
        return None;
    }
    let steamapps = comun.parent()?;
    for e in std::fs::read_dir(steamapps).ok()?.flatten() {
        let p = e.path();
        let nombre = p.file_name()?.to_str()?;
        if !nombre.starts_with("appmanifest_") || !nombre.ends_with(".acf") {
            continue;
        }
        let txt = std::fs::read_to_string(&p).ok()?;
        if crate::steam::valor_manifest(&txt, "installdir")
            .is_some_and(|d| d.eq_ignore_ascii_case(carpeta))
        {
            return crate::steam::valor_manifest(&txt, "appid");
        }
    }
    None
}

// El CDN público de Valve es el **último salto** de la cascada y lo resuelve la UI, que es
// quien sabe si está activado en Ajustes y puede reintentar con `onerror`. La función vivía
// también aquí, sin que nadie la llamara: dos copias de la misma URL en dos lenguajes, listas
// para desincronizarse el día que Valve cambie la ruta. Manda `urlCaratulaSteam` en `api.ts`.

// ── 3. Icono del ejecutable ──────────────────────────────────────────────────

fn u16le(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(off)?, *b.get(off + 1)?]))
}
fn u32le(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
    ]))
}

/// Tabla de secciones del PE, para traducir direcciones virtuales a desplazamientos de fichero.
struct Secciones(Vec<(u32, u32, u32)>); // (virtual_address, virtual_size, raw_offset)

impl Secciones {
    fn desplazamiento(&self, rva: u32) -> Option<usize> {
        self.0
            .iter()
            .find(|(va, vs, _)| rva >= *va && rva < va.saturating_add(*vs))
            .map(|(va, _, raw)| (rva - va + raw) as usize)
    }
}

/// Entrada de un directorio de recursos: `(id, desplazamiento, es_subdirectorio)`.
fn entradas_recurso(b: &[u8], dir: usize) -> Vec<(u32, usize, bool)> {
    let nombradas = u16le(b, dir + 12).unwrap_or(0) as usize;
    let numeradas = u16le(b, dir + 14).unwrap_or(0) as usize;
    let mut out = Vec::new();
    for i in 0..(nombradas + numeradas) {
        let e = dir + 16 + i * 8;
        let (Some(nombre), Some(offset)) = (u32le(b, e), u32le(b, e + 4)) else {
            break;
        };
        // El bit alto del nombre marca "es un nombre de texto"; solo usamos los numéricos.
        // El bit alto del offset marca "apunta a otro directorio".
        out.push((
            nombre & 0x7FFF_FFFF,
            (offset & 0x7FFF_FFFF) as usize,
            offset & 0x8000_0000 != 0,
        ));
    }
    out
}

/// Baja al primer nivel de idioma y devuelve `(rva_datos, tamaño)`.
fn hoja(b: &[u8], base: usize, mut dir: usize, profundidad: usize) -> Option<(u32, u32)> {
    for _ in 0..profundidad {
        let (_, off, es_dir) = *entradas_recurso(b, base + dir).first()?;
        if !es_dir {
            dir = off;
            break;
        }
        dir = off;
    }
    let e = base + dir;
    Some((u32le(b, e)?, u32le(b, e + 4)?))
}

/// Extrae el icono principal de un `.exe` y lo guarda como imagen suelta.
///
/// Se leen los recursos del PE a mano (sin GDI y sin crates de imagen): un `.ico` es
/// literalmente el `RT_GROUP_ICON` con sus `RT_ICON` pegados detrás. De ese `.ico` montado
/// en memoria se saca **el frame más grande**, que se escribe como `.png` o `.bmp`.
///
/// `destino` marca la ruta base; **la extensión real la decide el frame**, así que se
/// devuelve la ruta escrita y hay que usar esa, no la de entrada.
pub fn icono_de_exe(exe: &Path, destino: &Path) -> Option<PathBuf> {
    const RT_ICON: u32 = 3;
    const RT_GROUP_ICON: u32 = 14;

    let b = std::fs::read(exe).ok()?;
    if u16le(&b, 0)? != 0x5A4D {
        return None; // no empieza por "MZ"
    }
    let pe = u32le(&b, 0x3C)? as usize;
    if u32le(&b, pe)? != 0x0000_4550 {
        return None; // no es "PE\0\0"
    }
    let n_secciones = u16le(&b, pe + 6)? as usize;
    let tam_opcional = u16le(&b, pe + 20)? as usize;
    let opcional = pe + 24;
    // El directorio de datos empieza en distinto sitio según PE32 (0x10b) o PE32+ (0x20b).
    let dir_datos = opcional + if u16le(&b, opcional)? == 0x20b { 112 } else { 96 };
    let rsrc_rva = u32le(&b, dir_datos + 2 * 8)?;
    if rsrc_rva == 0 {
        return None; // el ejecutable no trae recursos
    }

    let mut secciones = Vec::with_capacity(n_secciones);
    for i in 0..n_secciones {
        let s = opcional + tam_opcional + i * 40;
        secciones.push((u32le(&b, s + 12)?, u32le(&b, s + 8)?, u32le(&b, s + 20)?));
    }
    let secciones = Secciones(secciones);
    let base = secciones.desplazamiento(rsrc_rva)?;

    // Nivel 1: tipos de recurso.
    let tipos = entradas_recurso(&b, base);
    let grupo = tipos.iter().find(|(id, _, d)| *id == RT_GROUP_ICON && *d)?;
    let iconos = tipos.iter().find(|(id, _, d)| *id == RT_ICON && *d)?;

    // Del grupo se coge el de menor id: por convención es el icono de la aplicación.
    let mut nombres = entradas_recurso(&b, base + grupo.1);
    nombres.sort_by_key(|(id, _, _)| *id);
    let (_, off_nombre, _) = *nombres.first()?;
    let (rva, _) = hoja(&b, base, off_nombre, 1)?;
    let dir_grupo = secciones.desplazamiento(rva)?;

    let cuenta = u16le(&b, dir_grupo + 4)? as usize;
    if cuenta == 0 {
        return None;
    }

    // Índice id → (offset, tamaño) de cada RT_ICON, para no recorrer el árbol N veces.
    let mut por_id = std::collections::HashMap::new();
    for (id, off, es_dir) in entradas_recurso(&b, base + iconos.1) {
        if !es_dir {
            continue;
        }
        if let Some((rva, tam)) = hoja(&b, base, off, 1) {
            if let Some(o) = secciones.desplazamiento(rva) {
                por_id.insert(id, (o, tam as usize));
            }
        }
    }

    // Reensamblado del .ico: cabecera + N entradas de 16 bytes + los datos en bruto.
    let mut cabecera = vec![0u8, 0, 1, 0];
    cabecera.extend_from_slice(&(cuenta as u16).to_le_bytes());
    let mut entradas = Vec::with_capacity(cuenta * 16);
    let mut datos = Vec::new();
    let mut offset = 6 + cuenta * 16;

    for i in 0..cuenta {
        let e = dir_grupo + 6 + i * 14;
        let id = u16le(&b, e + 12)? as u32;
        let Some(&(pos, tam)) = por_id.get(&id) else {
            continue;
        };
        let img = b.get(pos..pos + tam)?;
        // Cuidado con los dos formatos, que se parecen pero no son iguales:
        //   GRPICONDIRENTRY (14 B): …bits(2) · dwBytesInRes(4) · nID(2)
        //   ICONDIRENTRY    (16 B): …bits(2) · dwBytesInRes(4) · dwImageOffset(4)
        // Solo coinciden los **8 primeros** bytes. Copiar 12 metía `dwBytesInRes` por
        // duplicado y dejaba entradas de 20 B: la tabla quedaba desalineada y el fichero,
        // ilegible pese a tener una cabecera correcta.
        entradas.extend_from_slice(b.get(e..e + 8)?); // ancho, alto, colores, planos, bits
        entradas.extend_from_slice(&(tam as u32).to_le_bytes());
        entradas.extend_from_slice(&(offset as u32).to_le_bytes());
        datos.extend_from_slice(img);
        offset += tam;
    }
    if datos.is_empty() {
        return None;
    }
    // El nº real de entradas puede ser menor si algún RT_ICON no estaba.
    let reales = (entradas.len() / 16) as u16;
    cabecera[4..6].copy_from_slice(&reales.to_le_bytes());

    let mut ico = cabecera;
    ico.extend_from_slice(&entradas);
    ico.extend_from_slice(&datos);

    // No se escribe el `.ico`: se extrae **el frame más grande** y se guarda como imagen
    // normal. Dos razones: elegimos nosotros la resolución (un `.ico` trae de 16×16 a
    // 256×256 y el navegador no siempre coge la buena), y nos quitamos de encima la duda de
    // si el WebView pinta ICO multi-frame. Ver guión D.0_2.
    let (bytes, ext) = mejor_frame(&ico)?;
    let destino = destino.with_extension(ext);
    std::fs::create_dir_all(destino.parent()?).ok()?;
    std::fs::write(&destino, &bytes).ok()?;
    Some(destino)
}

/// Firma de un PNG: los iconos modernos guardan el frame de 256×256 ya en PNG.
const FIRMA_PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Extrae del `.ico` el frame de mayor área y lo devuelve como imagen suelta.
/// `(bytes, extensión)` — `png` si el frame ya venía en PNG, `bmp` si era un DIB.
fn mejor_frame(ico: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let cuenta = u16le(ico, 4)? as usize;
    let mut mejor: Option<(u32, u16, usize, usize)> = None; // (área, bits, offset, tamaño)
    for i in 0..cuenta {
        let e = 6 + i * 16;
        // 0 significa 256 en el formato ICO.
        let ancho = match *ico.get(e)? {
            0 => 256u32,
            w => w as u32,
        };
        let alto = match *ico.get(e + 1)? {
            0 => 256u32,
            h => h as u32,
        };
        let bits = u16le(ico, e + 6)?;
        let tam = u32le(ico, e + 8)? as usize;
        let off = u32le(ico, e + 12)? as usize;
        if ico.len() < off + tam {
            continue;
        }
        let clave = (ancho * alto, bits);
        if mejor.is_none_or(|(a, b, _, _)| clave > (a, b)) {
            mejor = Some((clave.0, clave.1, off, tam));
        }
    }
    let (_, _, off, tam) = mejor?;
    let datos = ico.get(off..off + tam)?;

    if datos.starts_with(FIRMA_PNG) {
        return Some((datos.to_vec(), "png"));
    }
    dib_a_bmp(datos).map(|b| (b, "bmp"))
}

/// Convierte el DIB de un frame de icono en un BMP autónomo.
///
/// Dos detalles del formato ICO que hay que deshacer: el DIB declara **el doble de alto**
/// (guarda la máscara AND debajo de la imagen), y le falta la cabecera de fichero.
fn dib_a_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let tam_cabecera = u32le(dib, 0)? as usize;
    if tam_cabecera < 40 || dib.len() < tam_cabecera {
        return None;
    }
    let alto_doble = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bits = u16le(dib, 14)?;
    let colores_usados = u32le(dib, 32)? as usize;
    let paleta = if bits <= 8 {
        let n = if colores_usados > 0 {
            colores_usados
        } else {
            1usize << bits
        };
        n * 4
    } else {
        0
    };

    let mut salida = Vec::with_capacity(14 + dib.len());
    let offset_datos = 14 + tam_cabecera + paleta;
    salida.extend_from_slice(b"BM");
    salida.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    salida.extend_from_slice(&0u32.to_le_bytes()); // reservado
    salida.extend_from_slice(&(offset_datos as u32).to_le_bytes());
    salida.extend_from_slice(dib);
    // Alto real = la mitad del declarado (la otra mitad es la máscara AND).
    let alto = alto_doble / 2;
    salida[14 + 8..14 + 12].copy_from_slice(&alto.to_le_bytes());
    Some(salida)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_fichero_que_no_es_pe_no_revienta() {
        let tmp = std::env::temp_dir().join(format!("sc_noexe_{}.exe", std::process::id()));
        std::fs::write(&tmp, b"esto no es un ejecutable").unwrap();
        let destino = std::env::temp_dir().join("sc_icono_test.ico");
        assert!(icono_de_exe(&tmp, &destino).is_none());
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn sin_cache_de_steam_no_devuelve_nada() {
        let vacio = std::env::temp_dir().join("sc_steam_inexistente");
        assert!(caratula_steam_local(&vacio, "228380").is_none());
    }

    /// **Regresión del fallo de las carátulas.** No basta con que el fichero exista: se
    /// comprueba que sea una imagen real con cabecera de PNG o de BMP y un tamaño coherente.
    /// El bug original (entradas de 20 B en vez de 16 en el `.ico` intermedio) producía
    /// ficheros con cabecera válida pero ilegibles, y una comprobación laxa no lo detectó.
    #[test]
    fn la_salida_es_una_imagen_decodificable() {
        let destino_base = std::env::temp_dir().join(format!("sc_reg_{}", std::process::id()));
        let mut comprobados = 0;
        for exe in [
            r"C:\Windows\System32
otepad.exe",
            r"C:\Windows\explorer.exe",
        ] {
            let p = Path::new(exe);
            if !p.is_file() {
                continue;
            }
            let Some(salida) = icono_de_exe(p, &destino_base.join("x.ico")) else {
                continue;
            };
            let b = std::fs::read(&salida).unwrap();
            let ext = salida.extension().and_then(|x| x.to_str()).unwrap_or("");
            match ext {
                "png" => assert!(b.starts_with(FIRMA_PNG), "PNG con firma válida"),
                "bmp" => {
                    assert!(b.starts_with(b"BM"), "BMP con firma válida");
                    // El offset a los píxeles debe caer dentro del fichero.
                    let off = u32le(&b, 10).unwrap() as usize;
                    assert!(off > 14 && off < b.len(), "offset de datos coherente: {off}");
                    // Y el alto ya no puede ser el doble que declara el icono.
                    let alto = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
                    let ancho = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
                    assert_eq!(alto, ancho, "el alto se corrige a la mitad: {ancho}x{alto}");
                }
                otro => panic!("extensión inesperada: {otro}"),
            }
            assert!(b.len() > 500, "una imagen de verdad no ocupa 4 bytes");
            comprobados += 1;
        }
        assert!(comprobados > 0, "debería poder leer algún ejecutable del sistema");
        std::fs::remove_dir_all(&destino_base).ok();
    }

    /// Contra los ejecutables reales del sistema. Ignorado por defecto.
    /// `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn extrae_iconos_de_ejecutables_reales() {
        let destino_base = std::env::temp_dir().join("sc_iconos");
        let candidatos = [
            r"C:\Windows\System32\notepad.exe",
            r"C:\Windows\explorer.exe",
            r"C:\Windows\System32\mspaint.exe",
        ];
        let mut extraidos = 0;
        for (i, exe) in candidatos.iter().enumerate() {
            let p = Path::new(exe);
            if !p.is_file() {
                continue;
            }
            let destino = destino_base.join(format!("{i}.ico"));
            match icono_de_exe(p, &destino) {
                Some(_) => {
                    let tam = std::fs::metadata(&destino).unwrap().len();
                    println!("{exe} → {tam} bytes");
                    // Un .ico válido empieza por 00 00 01 00.
                    let b = std::fs::read(&destino).unwrap();
                    assert_eq!(&b[0..4], &[0, 0, 1, 0], "cabecera de .ico válida");
                    assert!(tam > 100, "el icono no puede estar vacío");
                    extraidos += 1;
                }
                None => println!("{exe} → sin icono"),
            }
        }
        assert!(extraidos > 0, "debería extraer al menos un icono del sistema");
        std::fs::remove_dir_all(&destino_base).ok();
    }
}

#[cfg(test)]
mod diagnostico {
    use super::*;

    /// Comprueba la cascada contra la biblioteca real, sin escribir en la BD.
    /// `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cascada_contra_biblioteca_real() {
        let db = dirs_datos().join("library.db");
        if !db.is_file() {
            println!("no existe {}", db.display());
            return;
        }
        let conn = rusqlite::Connection::open(&db).unwrap();
        let steam = crate::steam::carpeta_steam();
        let tmp = std::env::temp_dir().join("sc_cascada");

        let mut st = conn
            .prepare("SELECT id, title, exe_path, install_dir, store_id FROM game WHERE state <> 'excluded'")
            .unwrap();
        let filas: Vec<(i64, String, Option<String>, Option<String>, Option<String>)> = st
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .filter_map(|x| x.ok())
            .collect();

        let (mut deducidos, mut por_steam, mut por_icono, mut png, mut bmp, mut nada) =
            (0, 0, 0, 0, 0, 0);
        for (id, titulo, exe, dir, appid) in &filas {
            let appid = appid.clone().filter(|a| !a.is_empty()).or_else(|| {
                let a = dir.as_deref().and_then(|d| appid_desde_carpeta(Path::new(d)));
                if a.is_some() {
                    deducidos += 1;
                }
                a
            });
            let steam_hit = match (&appid, &steam) {
                (Some(a), Some(s)) => caratula_steam_local(s, a),
                _ => None,
            };
            if steam_hit.is_some() {
                por_steam += 1;
                continue;
            }
            match exe.as_deref().filter(|e| !e.is_empty()).and_then(|e| {
                icono_de_exe(Path::new(e), &tmp.join(format!("{id}.ico")))
            }) {
                Some(p) => {
                    por_icono += 1;
                    match p.extension().and_then(|x| x.to_str()) {
                        Some("png") => png += 1,
                        Some("bmp") => bmp += 1,
                        _ => {}
                    }
                }
                None => {
                    nada += 1;
                    if nada <= 5 {
                        println!("  sin nada: [{id}] {titulo}");
                    }
                }
            }
        }
        println!(
            "\ntotal={} · appids deducidos={deducidos} · por caché de Steam={por_steam} · \
             por icono={por_icono} (png={png} bmp={bmp}) · sin nada={nada}",
            filas.len()
        );
        std::fs::remove_dir_all(&tmp).ok();
        assert!(por_steam + por_icono > 0, "algo debería resolverse");
    }

    /// Deja 8 imágenes en `%TEMP%\sc_muestra` para poder abrirlas con un visor y comprobar
    /// que son imágenes de verdad. `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn deja_una_muestra_para_inspeccionar() {
        let db = dirs_datos().join("library.db");
        if !db.is_file() {
            return;
        }
        let conn = rusqlite::Connection::open(&db).unwrap();
        let destino = std::env::temp_dir().join("sc_muestra");
        std::fs::create_dir_all(&destino).ok();
        let mut st = conn
            .prepare(
                "SELECT id, exe_path FROM game
                 WHERE COALESCE(exe_path,'') <> '' AND state <> 'excluded' LIMIT 8",
            )
            .unwrap();
        let filas: Vec<(i64, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .filter_map(|x| x.ok())
            .collect();
        for (id, exe) in filas {
            match icono_de_exe(Path::new(&exe), &destino.join(format!("{id}.ico"))) {
                Some(p) => println!("{}", p.display()),
                None => println!("[{id}] sin icono"),
            }
        }
        println!("muestra en: {}", destino.display());
    }

    fn dirs_datos() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("com.sofacervecero.hub")
    }
}
