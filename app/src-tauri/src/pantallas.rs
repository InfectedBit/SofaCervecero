//! Inventario de pantallas conectadas (guión E.0_4, fase 1).
//!
//! Se hace con Win32 directamente y **no** a través de Tauri a propósito: la lista hace falta
//! también **con la ventana cerrada**, que es el caso de «mantener el botón Xbox para entrar en
//! Modo Sofá» — ahí todavía no hay ninguna ventana a la que preguntarle.
//!
//! Todo lo que sale de aquí va en **píxeles físicos**. Es la única forma de que cuadre con un
//! televisor al 250 % de escala: un proceso sin conciencia de DPI ve el de este equipo como
//! 1638×864 en vez de 4096×2160, y cualquier cálculo de posición le sale con un error de ×2,5.
//! La app es `PER_MONITOR_AWARE` (lo pone `tao`), así que aquí las coordenadas ya son reales.

#![cfg(windows)]

use serde::Serialize;

/// Una pantalla conectada y activa.
#[derive(Debug, Clone, Serialize)]
pub struct Pantalla {
    /// Nombre de dispositivo GDI: `\\.\DISPLAY1`. Es lo que entiende `ChangeDisplaySettingsEx`.
    pub id: String,
    /// Nombre que entiende una persona: «LG TV SSCR2». Si el sistema no lo sabe, el `id`.
    pub nombre: String,
    pub x: i32,
    pub y: i32,
    /// Resolución **física**.
    pub ancho: u32,
    pub alto: u32,
    /// 1.0 = 100 %, 2.5 = 250 %. Sale del DPI efectivo del monitor.
    pub escala: f64,
    pub primaria: bool,
}

#[cfg(windows)]
mod win {
    use super::Pantalla;
    use std::collections::HashMap;
    use windows_sys::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        QDC_ONLY_ACTIVE_PATHS, SetDisplayConfig, SDC_ALLOW_CHANGES, SDC_APPLY,
        SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND, SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE,
    };

    /// La ruta está en uso. No lo exporta esta versión de `windows-sys`; es constante de la API.
    const DISPLAYCONFIG_PATH_ACTIVE: u32 = 0x0000_0001;
    /// «Esta ruta no apunta a ningún modo»: lo que hay que poner al desactivarla.
    const DISPLAYCONFIG_PATH_MODE_IDX_INVALID: u32 = 0xffff_ffff;
    use windows_sys::Win32::Foundation::{BOOL, ERROR_SUCCESS, LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };

    /// `MONITORINFOF_PRIMARY`. No lo expone esta versión de `windows-sys`, y es una constante de
    /// la API que lleva igual desde Windows 2000.
    const PRIMARIA: u32 = 1;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    /// Convierte un buffer de `u16` terminado en `\0` a `String`.
    fn texto(buf: &[u16]) -> String {
        let fin = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..fin])
    }

    /// Nombre amigable de cada pantalla, indexado por su nombre GDI (`\\.\DISPLAY1`).
    ///
    /// `EnumDisplayMonitors` no lo da: solo conoce el nombre del *adaptador*. El nombre que lee
    /// la gente —«LG TV SSCR2»— está en la configuración de pantallas (`QueryDisplayConfig`),
    /// que empareja cada *source* (la salida de la gráfica) con su *target* (el monitor físico).
    fn nombres_amigables() -> HashMap<String, String> {
        let mut out = HashMap::new();
        unsafe {
            let (mut n_rutas, mut n_modos) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_rutas, &mut n_modos)
                != ERROR_SUCCESS
            {
                return out;
            }
            let mut rutas: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); n_rutas as usize];
            let mut modos: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); n_modos as usize];
            if QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut n_rutas,
                rutas.as_mut_ptr(),
                &mut n_modos,
                modos.as_mut_ptr(),
                std::ptr::null_mut(),
            ) != ERROR_SUCCESS
            {
                return out;
            }

            for ruta in rutas.iter().take(n_rutas as usize) {
                // El lado "source" nos da el `\\.\DISPLAYn` con el que correlacionar.
                let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
                src.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: ruta.sourceInfo.adapterId,
                    id: ruta.sourceInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                    continue;
                }
                // El lado "target" nos da el nombre del monitor.
                let mut tgt: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
                tgt.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: ruta.targetInfo.adapterId,
                    id: ruta.targetInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut tgt.header) != 0 {
                    continue;
                }
                let gdi = texto(&src.viewGdiDeviceName);
                let amigable = texto(&tgt.monitorFriendlyDeviceName);
                if !gdi.is_empty() && !amigable.is_empty() {
                    out.insert(gdi, amigable);
                }
            }
        }
        out
    }

    unsafe extern "system" fn recoger(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        datos: LPARAM,
    ) -> BOOL {
        // `datos` es el `&mut Vec<Pantalla>` que le pasamos a `EnumDisplayMonitors`; es válido
        // durante toda la enumeración, que es síncrona.
        unsafe {
            let lista = &mut *(datos as *mut Vec<Pantalla>);

            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) == 0 {
                return TRUE;
            }
            let r = info.monitorInfo.rcMonitor;

            // DPI efectivo del monitor: 96 = 100 %, 240 = 250 %.
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);

            let id = texto(&info.szDevice);
            lista.push(Pantalla {
                nombre: id.clone(),
                id,
                x: r.left,
                y: r.top,
                ancho: (r.right - r.left).max(0) as u32,
                alto: (r.bottom - r.top).max(0) as u32,
                escala: dpi_x as f64 / 96.0,
                primaria: info.monitorInfo.dwFlags & PRIMARIA != 0,
            });
        }
        TRUE
    }

    // ── Reconfigurar el escritorio (guión E.0_4 fase 3) ──────────────────────────────────
    //
    // Todo esto va con el patrón que exige Win32 y que es fácil hacer mal: se preparan **todos**
    // los cambios con `CDS_NORESET` —que los deja escritos pero sin aplicar— y al final se hace
    // **una sola** llamada con parámetros nulos que los aplica de golpe. Hacerlo monitor a
    // monitor provoca parpadeos, estados intermedios inválidos y, con suerte, un fallo.

    use windows_sys::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, EnumDisplayDevicesW, EnumDisplaySettingsExW, CDS_NORESET,
        CDS_SET_PRIMARY, CDS_UPDATEREGISTRY, DEVMODEW, DISPLAY_DEVICEW,
        DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFLAGS, DM_DISPLAYFREQUENCY,
        DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION,
        ENUM_CURRENT_SETTINGS,
    };

    /// `DISPLAY_DEVICE_ATTACHED_TO_DESKTOP`: la pantalla forma parte del escritorio ahora mismo.
    const ENCENDIDA: u32 = 0x0000_0001;

    /// Todos los adaptadores de pantalla del sistema, encendidos o no.
    fn dispositivos() -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let mut i = 0u32;
            loop {
                let mut d: DISPLAY_DEVICEW = std::mem::zeroed();
                d.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
                if EnumDisplayDevicesW(std::ptr::null(), i, &mut d, 0) == 0 {
                    break;
                }
                out.push(texto(&d.DeviceName));
                i += 1;
            }
        }
        out
    }

    /// Modo actual de una pantalla. `None` si el nombre no existe.
    fn modo_actual(nombre: &str) -> Option<DEVMODEW> {
        let ancho: Vec<u16> = nombre.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut dm: DEVMODEW = std::mem::zeroed();
            dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            if EnumDisplaySettingsExW(ancho.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm, 0) == 0 {
                return None;
            }
            Some(dm)
        }
    }

    fn encendida(nombre: &str) -> bool {
        let ancho: Vec<u16> = nombre.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut i = 0u32;
            loop {
                let mut d: DISPLAY_DEVICEW = std::mem::zeroed();
                d.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
                if EnumDisplayDevicesW(std::ptr::null(), i, &mut d, 0) == 0 {
                    return false;
                }
                if texto(&d.DeviceName) == nombre {
                    return d.StateFlags & ENCENDIDA != 0;
                }
                i += 1;
                let _ = &ancho;
            }
        }
    }

    /// Traduce el código de `ChangeDisplaySettingsEx` a algo que se pueda leer.
    fn por_que(r: i32) -> &'static str {
        match r {
            -1 => "el controlador rechazó la disposición",
            -2 => "modo de pantalla no admitido",
            -3 => "hace falta reiniciar",
            -4 => "combinación de opciones inválida",
            -5 => "parámetro inválido",
            -6 => "no se pudo escribir en el registro",
            _ => "motivo desconocido",
        }
    }

    /// Escribe el cambio de una pantalla **sin aplicarlo** todavía.
    fn preparar(nombre: &str, dm: &mut DEVMODEW, extra: u32) -> Result<(), String> {
        let ancho: Vec<u16> = nombre.encode_utf16().chain(std::iter::once(0)).collect();
        let r = unsafe {
            ChangeDisplaySettingsExW(
                ancho.as_ptr(),
                dm,
                std::ptr::null_mut(),
                CDS_UPDATEREGISTRY | CDS_NORESET | extra,
                std::ptr::null(),
            )
        };
        if r == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(format!("{nombre}: {} (código {r})", por_que(r)))
        }
    }

    /// Deja una pantalla preparada para **desconectarse**.
    ///
    /// Desconectar no es «poner el modo actual con el tamaño a cero»: es mandar un `DEVMODEW`
    /// **entero a cero**, con `dmSize` y `dmFields` como único contenido. Reutilizar el modo
    /// vigente y parchearle el ancho y el alto deja dentro la frecuencia de refresco y los
    /// `dmDisplayFlags` de antes, y Windows los valida contra un modo de 0×0: contesta
    /// `DISP_CHANGE_BADMODE`, que es exactamente el error que daba.
    fn preparar_apagado(nombre: &str) -> Result<(), String> {
        let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        dm.dmFields = DM_POSITION
            | DM_PELSWIDTH
            | DM_PELSHEIGHT
            | DM_BITSPERPEL
            | DM_DISPLAYFREQUENCY
            | DM_DISPLAYFLAGS;
        preparar(nombre, &mut dm, 0)
    }

    /// Deja la posición de una pantalla preparada, **sin tocar su modo de vídeo**.
    ///
    /// Solo `DM_POSITION`: pedir además resolución y profundidad de color es pedirle al
    /// controlador que valide un modo que no se está cambiando, y es una forma fácil de que
    /// conteste que no.
    fn preparar_posicion(nombre: &str, x: i32, y: i32, extra: u32) -> Result<(), String> {
        let Some(mut dm) = modo_actual(nombre) else {
            return Ok(());
        };
        dm.dmFields = DM_POSITION;
        dm.Anonymous1.Anonymous2.dmPosition.x = x;
        dm.Anonymous1.Anonymous2.dmPosition.y = y;
        preparar(nombre, &mut dm, extra)
    }

    /// Aplica de golpe todo lo preparado.
    fn aplicar_todo() -> Result<(), String> {
        let r = unsafe {
            ChangeDisplaySettingsExW(
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if r == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(format!("el sistema rechazó la nueva disposición (código {r})"))
        }
    }

    /// `nombre|x|y|ancho|alto|encendida` por pantalla, separadas por `;`.
    pub fn fotografiar() -> String {
        dispositivos()
            .into_iter()
            .filter_map(|n| {
                let dm = modo_actual(&n)?;
                let p = unsafe { dm.Anonymous1.Anonymous2.dmPosition };
                Some(format!(
                    "{}|{}|{}|{}|{}|{}",
                    n,
                    p.x,
                    p.y,
                    dm.dmPelsWidth,
                    dm.dmPelsHeight,
                    if encendida(&n) { 1 } else { 0 }
                ))
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    pub fn restaurar(foto: &str) -> Result<(), String> {
        if foto.trim().is_empty() {
            return Ok(());
        }
        // Si la foto tiene más pantallas encendidas de las que hay ahora, es que venimos de un
        // «dejar solo esta»: hay que **reactivarlas** antes de colocarlas. Devolver posiciones a
        // una pantalla desconectada no hace nada.
        let encendidas_foto = foto
            .split(';')
            .filter(|l| l.split('|').nth(5) == Some("1"))
            .count();
        if encendidas_foto > dispositivos().iter().filter(|d| encendida(d)).count() {
            volver_a_extender()?;
        }

        // La que vuelve a ser primaria —la que estaba en (0,0)— va **primero**, por el mismo
        // motivo que al aplicar: mientras nadie ocupa el origen, el resto no se puede colocar.
        let mut lineas: Vec<&str> = foto.split(';').collect();
        lineas.sort_by_key(|l| {
            let p: Vec<&str> = l.split('|').collect();
            let en_origen = p.len() == 6 && p[1] == "0" && p[2] == "0" && p[5] == "1";
            if en_origen { 0 } else { 1 }
        });

        for linea in lineas {
            let p: Vec<&str> = linea.split('|').collect();
            if p.len() != 6 {
                continue;
            }
            let (nombre, x, y, w, h, on) = (
                p[0],
                p[1].parse::<i32>().unwrap_or(0),
                p[2].parse::<i32>().unwrap_or(0),
                p[3].parse::<u32>().unwrap_or(0),
                p[4].parse::<u32>().unwrap_or(0),
                p[5] == "1",
            );
            if !on {
                preparar_apagado(nombre)?;
                continue;
            }
            let Some(mut dm) = modo_actual(nombre) else {
                continue;
            };
            dm.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL;
            // Escribir en una unión es seguro; solo leerla necesita `unsafe`.
            dm.Anonymous1.Anonymous2.dmPosition.x = x;
            dm.Anonymous1.Anonymous2.dmPosition.y = y;
            dm.dmPelsWidth = w;
            dm.dmPelsHeight = h;
            // La que estaba en (0,0) es la primaria: hay que decírselo explícitamente.
            let extra = if x == 0 && y == 0 { CDS_SET_PRIMARY } else { 0 };
            preparar(nombre, &mut dm, extra)?;
        }
        aplicar_todo()
    }

    /// **B** — hace primaria la pantalla indicada.
    ///
    /// No basta con marcarla: la primaria **define el origen**, así que a todas las demás hay
    /// que restarles su posición. Si no se hace, Windows rechaza la disposición o deja huecos.
    pub fn hacer_primaria(destino: &str) -> Result<(), String> {
        let dm_destino = modo_actual(destino).ok_or_else(|| format!("no existe {destino}"))?;
        let origen = unsafe { dm_destino.Anonymous1.Anonymous2.dmPosition };
        if origen.x == 0 && origen.y == 0 {
            return Ok(()); // ya es la primaria
        }

        // **El destino va primero, y no es un detalle de estilo.** La primaria *define* el
        // origen del escritorio, así que mover otra pantalla fuera de (0,0) mientras ninguna
        // ocupa ese sitio deja una disposición sin origen, y el controlador la rechaza con un
        // `DISP_CHANGE_FAILED` seco. Fijando antes quién manda, el resto son solo offsets.
        preparar_posicion(destino, 0, 0, CDS_SET_PRIMARY)?;

        for nombre in dispositivos() {
            if nombre == destino || !encendida(&nombre) {
                continue;
            }
            let Some(dm) = modo_actual(&nombre) else {
                continue;
            };
            let p = unsafe { dm.Anonymous1.Anonymous2.dmPosition };
            preparar_posicion(&nombre, p.x - origen.x, p.y - origen.y, 0)?;
        }
        aplicar_todo()
    }

    /// **C** — deja encendida **solo** la pantalla indicada.
    ///
    /// Se apagan las demás poniéndoles tamaño 0, que es como Windows desconecta una pantalla sin
    /// olvidarla. Y la que queda pasa a (0,0): si el escritorio se queda sin origen, el sistema
    /// rechaza el cambio entero.
    /// **C — dejar encendida solo la pantalla indicada.**
    ///
    /// Va con `SetDisplayConfig`, no con `ChangeDisplaySettingsEx`, y eso costó dos intentos
    /// fallidos antes de medirlo. La receta clásica para desconectar —un `DEVMODE` con el tamaño
    /// a cero— **no funciona en este sistema**: probada con `CDS_UPDATEREGISTRY | CDS_NORESET`
    /// sobre un monitor *secundario*, y con cuatro combinaciones distintas de `dmFields`, las
    /// cuatro contestaron `DISP_CHANGE_BADMODE`. No era el orden ni la primaria: es que esa vía
    /// está muerta.
    ///
    /// `SetDisplayConfig` es la API moderna de topología, y expresa justo lo que se quiere:
    /// *«de todas las rutas que hay, deja activa solo esta»*. Se valida antes de aplicar, para
    /// poder dar un error claro en vez de dejar el escritorio a medias.
    pub fn solo_esta(destino: &str) -> Result<(), String> {
        // `QDC_ONLY_ACTIVE_PATHS` y no `QDC_ALL_PATHS`: con «todas» vienen decenas de rutas
        // posibles —cada salida contra cada destino al que *podría* conectarse, muchas con el
        // destino ni siquiera disponible—, y `SetDisplayConfig` valida el array **entero**.
        // Con solo las activas son tres entradas, consistentes por construcción, y lo único
        // que hay que hacer es quitarle el flag a las que sobran.
        // Los modos se descartan: esta llamada va sin ellos (ver más abajo).
        let (mut rutas, _) = configuracion(QDC_ONLY_ACTIVE_PATHS)?;
        let gdi = por_ruta(&rutas);

        // **Solo se desactivan las demás; la del destino no se toca.**
        //
        // Activarla parecía lo natural y era justo el error: con `QDC_ALL_PATHS` una misma
        // salida puede tener **varias rutas** —una por cada destino al que podría conectarse—,
        // y marcarlas todas como activas describe una clonación imposible. De ahí el
        // `ERROR_INVALID_PARAMETER` (87) en la validación. La pantalla elegida ya está activa,
        // que para eso está conectada: no hay nada que encender.
        let mut sigue_viva = false;
        for (i, r) in rutas.iter_mut().enumerate() {
            let activa = r.flags & DISPLAYCONFIG_PATH_ACTIVE != 0;
            if gdi.get(&i).map(|n| n == destino).unwrap_or(false) {
                sigue_viva |= activa;
            } else if activa {
                r.flags &= !DISPLAYCONFIG_PATH_ACTIVE;
            }
            // **Todas** las rutas se quedan sin índice de modo, también la que sobrevive, y la
            // llamada va **sin array de modos**: cada ruta dice «no apunto a ninguno» y Windows
            // elige los que correspondan.
            //
            // Es la clave que faltaba, y la dio el sondeo (`sondeo_solo_esta`). Conservar el
            // modo de la pantalla que sobrevive devolvía `ERROR_INVALID_PARAMETER` (87) para la
            // secundaria y el televisor, pero **funcionaba para DISPLAY1**. Esa asimetría es
            // toda la explicación: DISPLAY1 es la primaria, está en (0,0), y una pantalla que se
            // va a quedar sola tiene que estar en el origen. El modo de origen que sobrevivía
            // seguía describiendo su posición dentro del escritorio múltiple —un desplazamiento
            // que ya no existe cuando no hay nada a su izquierda—, y eso es la configuración
            // imposible. No eran modos huérfanos: era una posición imposible.
            //
            // Recortar y renumerar el array tampoco bastaba, por lo mismo. Dejar que Windows
            // calcule los modos es más simple que recolocar a mano lo que él sabe deducir.
            r.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
            r.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        }
        if !sigue_viva {
            return Err(format!("{destino} no está activa ahora mismo"));
        }

        let mut aplicar = |flags: u32| -> i32 {
            unsafe {
                SetDisplayConfig(
                    rutas.len() as u32,
                    rutas.as_mut_ptr(),
                    0,
                    std::ptr::null_mut(),
                    flags,
                )
            }
        };
        // Validar **no** lleva `SDC_SAVE_TO_DATABASE`: guardar sin aplicar no significa nada y
        // la combinación se rechaza. `SDC_ALLOW_CHANGES` no hace falta —sin modos el sondeo
        // valida igual con y sin él—, pero se manda porque es lo que dice en voz alta lo que se
        // está pidiendo: ajusta tú lo que no te he especificado.
        let base = SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES;
        let v = aplicar(base | SDC_VALIDATE);
        if v != ERROR_SUCCESS as i32 {
            // Con el diagnóstico dentro, un fallo se depura sin otra vuelta de preguntas.
            let activas = rutas
                .iter()
                .filter(|r| r.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
                .count();
            return Err(format!(
                "el sistema no admite dejar solo {destino}                  (validación: {v}; {} rutas, {} activas tras el cambio)",
                rutas.len(),
                activas
            ));
        }
        let r = aplicar(base | SDC_APPLY | SDC_SAVE_TO_DATABASE);
        if r != ERROR_SUCCESS as i32 {
            return Err(format!("no se pudo dejar solo {destino} (código {r})"));
        }
        Ok(())
    }

    /// Vuelve a encender todas las pantallas, en modo extendido.
    ///
    /// Es el inverso de `solo_esta`, y hace falta porque devolver las posiciones una a una no
    /// sirve si la pantalla sigue desconectada. La topología se recupera de la base de datos de
    /// Windows, que es donde quedó guardada; luego `restaurar()` repone las posiciones exactas.
    pub fn volver_a_extender() -> Result<(), String> {
        let r = unsafe {
            SetDisplayConfig(
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                SDC_TOPOLOGY_EXTEND | SDC_APPLY,
            )
        };
        if r == ERROR_SUCCESS as i32 {
            Ok(())
        } else {
            Err(format!("no se pudieron reactivar las pantallas (código {r})"))
        }
    }

    /// Lee la configuración de pantallas completa (rutas + modos).
    fn configuracion(que: u32) -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), String> {
        unsafe {
            let (mut n_rutas, mut n_modos) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(que, &mut n_rutas, &mut n_modos) != ERROR_SUCCESS {
                return Err("no se pudo leer la configuración de pantallas".into());
            }
            let mut rutas = vec![std::mem::zeroed(); n_rutas as usize];
            let mut modos = vec![std::mem::zeroed(); n_modos as usize];
            if QueryDisplayConfig(
                que,
                &mut n_rutas,
                rutas.as_mut_ptr(),
                &mut n_modos,
                modos.as_mut_ptr(),
                std::ptr::null_mut(),
            ) != ERROR_SUCCESS
            {
                return Err("no se pudo leer la configuración de pantallas".into());
            }
            rutas.truncate(n_rutas as usize);
            modos.truncate(n_modos as usize);
            Ok((rutas, modos))
        }
    }

    /// Nombre GDI (`\\.\DISPLAY1`) de cada ruta, por índice.
    fn por_ruta(rutas: &[DISPLAYCONFIG_PATH_INFO]) -> HashMap<usize, String> {
        let mut out = HashMap::new();
        for (i, ruta) in rutas.iter().enumerate() {
            unsafe {
                let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
                src.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: ruta.sourceInfo.adapterId,
                    id: ruta.sourceInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut src.header) == 0 {
                    out.insert(i, texto(&src.viewGdiDeviceName));
                }
            }
        }
        out
    }

    pub fn listar() -> Vec<Pantalla> {
        let mut lista: Vec<Pantalla> = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(recoger),
                &mut lista as *mut _ as LPARAM,
            );
        }
        let amigables = nombres_amigables();
        for p in &mut lista {
            if let Some(n) = amigables.get(&p.id) {
                p.nombre = n.clone();
            }
        }
        // Izquierda a derecha, arriba a abajo: el mismo orden en que se ven en el escritorio,
        // que es como hay que pintarlas en el selector.
        lista.sort_by_key(|p| (p.x, p.y));
        lista
    }

    /// Sondeo de las variantes de `solo_esta`, **sin aplicar ninguna**.
    ///
    /// No es una prueba de regresión: es un instrumento de medida, y por eso está `#[ignore]`.
    /// `SDC_VALIDATE` no toca el escritorio, así que se pueden comparar todas las formas de
    /// describir «deja activa solo esta pantalla» y ver qué contesta Windows a cada una, en vez
    /// de ir cambiando el código a ciegas y preguntar.
    ///
    /// `cargo test -- --ignored --nocapture sondeo_solo_esta`
    #[test]
    #[ignore = "instrumento de medida; depende de las pantallas de la máquina"]
    fn sondeo_solo_esta() {
        for destino in listar().iter().map(|p| p.id.clone()).collect::<Vec<_>>() {
            println!("\n=== dejar solo {destino} ===");
            // Cada variante parte de una lectura limpia: las anteriores modifican su copia.
            let preparar = || -> (Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>) {
                let (mut rutas, modos) = configuracion(QDC_ONLY_ACTIVE_PATHS).unwrap();
                let gdi = por_ruta(&rutas);
                for (i, r) in rutas.iter_mut().enumerate() {
                    if !gdi.get(&i).map(|n| *n == destino).unwrap_or(false) {
                        r.flags &= !DISPLAYCONFIG_PATH_ACTIVE;
                    }
                }
                (rutas, modos)
            };
            let validar = |rutas: &mut Vec<DISPLAYCONFIG_PATH_INFO>,
                           modos: Option<&mut Vec<DISPLAYCONFIG_MODE_INFO>>,
                           flags: u32| -> i32 {
                let (n_m, p_m) = match modos {
                    Some(m) => (m.len() as u32, m.as_mut_ptr()),
                    None => (0, std::ptr::null_mut()),
                };
                unsafe {
                    SetDisplayConfig(rutas.len() as u32, rutas.as_mut_ptr(), n_m, p_m,
                                     flags | SDC_VALIDATE)
                }
            };
            let base = SDC_USE_SUPPLIED_DISPLAY_CONFIG;
            let libre = base | SDC_ALLOW_CHANGES;

            // A y B: sin array de modos, con todos los índices invalidados.
            for (nombre, flags) in [("A sin modos + ALLOW_CHANGES", libre),
                                    ("B sin modos, exacto", base)] {
                let (mut rutas, _) = preparar();
                for r in rutas.iter_mut() {
                    r.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                    r.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                }
                println!("  {nombre:38} -> {}", validar(&mut rutas, None, flags));
            }

            // C: array completo, pero con todos los índices invalidados.
            let (mut rutas, mut modos) = preparar();
            for r in rutas.iter_mut() {
                r.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                r.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
            }
            println!("  {:38} -> {}", "C modos, todos los índices invalid",
                     validar(&mut rutas, Some(&mut modos), libre));

            // D: lo que había antes — solo se invalidan los índices de las apagadas.
            let (mut rutas, mut modos) = preparar();
            for r in rutas.iter_mut() {
                if r.flags & DISPLAYCONFIG_PATH_ACTIVE == 0 {
                    r.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                    r.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                }
            }
            println!("  {:38} -> {}", "D invalid solo en las apagadas",
                     validar(&mut rutas, Some(&mut modos), libre));

            // E: array recortado a los modos de la superviviente, con sus índices renumerados.
            let (mut rutas, modos) = preparar();
            let mut recortados: Vec<DISPLAYCONFIG_MODE_INFO> = Vec::new();
            for r in rutas.iter_mut() {
                if r.flags & DISPLAYCONFIG_PATH_ACTIVE == 0 {
                    r.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                    r.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
                    continue;
                }
                for idx in [
                    unsafe { &mut r.sourceInfo.Anonymous.modeInfoIdx as *mut u32 },
                    unsafe { &mut r.targetInfo.Anonymous.modeInfoIdx as *mut u32 },
                ] {
                    let viejo = unsafe { *idx };
                    if let Some(m) = modos.get(viejo as usize) {
                        recortados.push(*m);
                        unsafe { *idx = recortados.len() as u32 - 1 };
                    }
                }
            }
            println!("  {:38} -> {} ({} modos)", "E modos recortados y renumerados",
                     validar(&mut rutas, Some(&mut recortados), libre), recortados.len());
        }
    }
}

/// Las pantallas conectadas y activas, de izquierda a derecha.
pub fn listar() -> Vec<Pantalla> {
    win::listar()
}

/// Qué se le hace al escritorio al entrar en Modo Sofá (guión E.0_4 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reparto {
    /// No se toca nada. El hub se abre ahí y punto.
    Ninguna,
    /// **B**: el monitor elegido pasa a ser el principal. Casi todo abre en el principal.
    Primaria,
    /// **C**: se apagan las demás. Es la que funciona seguro, y la que más molesta.
    Solo,
}

/// Fotografía de cómo estaba el escritorio, para poder devolverlo **exactamente** a su sitio.
pub fn fotografiar() -> String {
    win::fotografiar()
}

/// Devuelve el escritorio a la fotografía dada. Es idempotente: restaurar dos veces no rompe.
pub fn restaurar(foto: &str) -> Result<(), String> {
    win::restaurar(foto)
}

/// Aplica el reparto pedido. Devuelve la fotografía **de antes**, que es lo que hay que
/// guardar para poder deshacerlo.
///
/// Si algo falla a mitad, **se deshace aquí mismo**. No es un detalle: los cambios se preparan
/// con `CDS_UPDATEREGISTRY | CDS_NORESET`, o sea que quedan **escritos en el registro aunque no
/// se apliquen**. Abandonar a medias dejaría una disposición a medio hacer esperando al
/// siguiente arranque.
pub fn aplicar(reparto: Reparto, destino: &str) -> Result<String, String> {
    let antes = fotografiar();
    let r = match reparto {
        Reparto::Ninguna => Ok(()),
        Reparto::Primaria => win::hacer_primaria(destino),
        Reparto::Solo => win::solo_esta(destino),
    };
    match r {
        Ok(()) => Ok(antes),
        Err(e) => {
            let _ = win::restaurar(&antes);
            Err(e)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::listar;

    /// Invariantes que tienen que cumplirse en cualquier equipo, sin saber cuál es.
    #[test]
    fn el_inventario_es_coherente() {
        let p = listar();
        assert!(!p.is_empty(), "siempre hay al menos una pantalla");
        assert_eq!(
            p.iter().filter(|x| x.primaria).count(),
            1,
            "Windows garantiza exactamente una primaria"
        );
        for x in &p {
            assert!(x.ancho > 0 && x.alto > 0, "{} sin tamaño", x.id);
            assert!(x.escala >= 0.5 && x.escala <= 6.0, "escala rara en {}", x.id);
            assert!(!x.nombre.is_empty());
        }
        // La primaria **no** tiene por qué estar en (0,0) cuando hay escalado de por medio,
        // pero sí es el origen del escritorio: ninguna otra puede empezar antes en las dos ejes.
        let pr = p.iter().find(|x| x.primaria).unwrap();
        assert_eq!((pr.x, pr.y), (0, 0), "la primaria define el origen");
    }

    /// Sondeo manual: `cargo test -- --ignored --nocapture ver_las_pantallas`.
    ///
    /// **Ojo con lo que imprime.** El binario de `cargo test` **no tiene conciencia de DPI** —eso
    /// lo configura `tao` al arrancar la app—, así que aquí Windows devuelve las coordenadas
    /// *virtualizadas*: un televisor 4096×2160 al 250 % sale como «1638×864, escala 100 %».
    /// No es un fallo del módulo; es la misma trampa que describe el guión E.0_4 §3, vista desde
    /// dentro. **Los números de verdad hay que mirarlos desde la app**, no desde aquí.
    #[test]
    #[ignore = "depende del equipo; para mirar, no para comprobar"]
    fn ver_las_pantallas() {
        for p in listar() {
            println!(
                "{:<14} {:>5}x{:<5} en ({:>6},{:>5})  escala {:>5.0}%  {}{}",
                p.id,
                p.ancho,
                p.alto,
                p.x,
                p.y,
                p.escala * 100.0,
                p.nombre,
                if p.primaria { "  [primaria]" } else { "" }
            );
        }
    }
}
