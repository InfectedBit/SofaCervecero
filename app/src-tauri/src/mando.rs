//! Sondeo del mando desde el **núcleo** (guión E.0_4 fase 4).
//!
//! La interfaz ya lee el mando con la Web Gamepad API, pero eso solo sirve **mientras hay
//! ventana** — y cerrar el hub destruye el WebView2, que es toda la arquitectura del proyecto.
//! Para que el mando pueda traer el hub de vuelta, o hablar con él con un juego a pantalla
//! completa delante, tiene que escuchar alguien que no se muera: el núcleo.
//!
//! **No es un vigilante permanente.** Solo corre mientras el núcleo esté en la bandeja; con la
//! app cerrada del todo no hay nada escuchando, y así debe ser.
//!
//! ### Por qué XInput a mano y no un `crate`
//!
//! El botón Xbox —el de encendido— **no lo expone la API pública de XInput**, y tampoco `gilrs`.
//! Está en `XInputGetStateEx`, que no tiene nombre exportado: se resuelve por el **ordinal 100**
//! de `xinput1_4.dll`. Son treinta líneas y ninguna dependencia nueva, porque `windows-sys` ya
//! estaba.
//!
//! ### Por qué mantener SELECT y no el botón Xbox
//!
//! Porque **mantener pulsado el botón Xbox apaga el mando**. Así que el botón Xbox se queda como
//! pulsación única —con una acción distinta según dónde estemos— y el gesto de mantener va a
//! SELECT, que además es remapeable.

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Cada cuánto se consulta un mando conectado.
const SONDEO: Duration = Duration::from_millis(100);
/// Cada cuánto se buscan mandos nuevos. Microsoft avisa de que preguntar por huecos vacíos es
/// **caro**, así que los desconectados se miran de uvas a peras y no en cada vuelta.
const REBUSCA: Duration = Duration::from_secs(2);

// Máscaras de `XINPUT_GAMEPAD.wButtons`.
const BOTON_BACK: u16 = 0x0020; // SELECT
const BOTON_GUIA: u16 = 0x0400; // el botón Xbox; solo lo devuelve `XInputGetStateEx`

/// Qué ha hecho el usuario con el mando. Lo interpreta quien escucha, no este módulo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesto {
    /// Pulsación única del botón Xbox.
    Xbox,
    /// Se está manteniendo el botón del Modo Sofá. `0.0`–`1.0`.
    Progreso(f32),
    /// **Se soltó antes de completarlo**, o sea: una pulsación corta de ese botón.
    ///
    /// No es «no ha pasado nada». Ese botón normalmente tiene otra acción asignada —de fábrica,
    /// la guía— y es **el núcleo quien decide** si la pulsación fue corta o larga. Si la interfaz
    /// disparara su acción al pulsar, la guía se abriría nada más empezar a mantener y se
    /// solaparía con el Modo Sofá. Por eso el botón se reserva entero aquí y se avisa al soltar.
    Corta,
    /// Se ha mantenido el tiempo necesario.
    Completado,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Gamepad {
    botones: u16,
    gatillo_izq: u8,
    gatillo_der: u8,
    stick_ix: i16,
    stick_iy: i16,
    stick_dx: i16,
    stick_dy: i16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Estado {
    paquete: u32,
    gamepad: Gamepad,
}

type GetState = unsafe extern "system" fn(u32, *mut Estado) -> u32;

/// Resuelve `XInputGetStateEx` (ordinal 100). Devuelve `None` si no hay XInput en el sistema.
fn cargar() -> Option<GetState> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    for dll in ["xinput1_4.dll\0", "xinput1_3.dll\0"] {
        let ancho: Vec<u16> = dll.encode_utf16().collect();
        let h = unsafe { LoadLibraryW(ancho.as_ptr()) };
        if h.is_null() {
            continue;
        }
        // El ordinal se pasa como puntero con el valor numérico: es lo que hace `MAKEINTRESOURCE`.
        let f = unsafe { GetProcAddress(h, 100 as *const u8) };
        if let Some(f) = f {
            return Some(unsafe { std::mem::transmute::<_, GetState>(f) });
        }
    }
    None
}

/// Botón que activa el Modo Sofá. Por defecto **SELECT**; se puede remapear, y por eso vive en
/// un atómico que la interfaz puede cambiar en caliente.
pub static BOTON_SOFA: AtomicU32 = AtomicU32::new(8);
/// Cómo se activa: `0` mantener, `1` una pulsación, `2` dos pulsaciones seguidas.
pub static GESTO: AtomicU32 = AtomicU32::new(0);
/// Cuánto hay que mantener, en milisegundos. Configurable: hay a quien un segundo le sobra.
/// Nunca por debajo de `GRACIA` + 200 ms, o el círculo no daría tiempo ni a verse.
pub static MANTENER_MS: AtomicU32 = AtomicU32::new(1200);
/// Escuchar el mando desde el núcleo. Apagarlo quita el hilo entero.
pub static ESCUCHA: AtomicBool = AtomicBool::new(true);
/// El botón Xbox trae el hub al frente. Se puede desactivar, por si estorba.
pub static XBOX_TRAE: AtomicBool = AtomicBool::new(true);
/// Exigir **dos pulsaciones seguidas** del botón Xbox. Para quien lo roce sin querer.
pub static XBOX_DOBLE: AtomicBool = AtomicBool::new(false);

/// Margen para encadenar dos pulsaciones y que cuenten como doble.
const DOBLE: Duration = Duration::from_millis(400);
/// Lo mismo para el botón Xbox, pero más holgado: **un segundo**. Ese botón se pulsa con el
/// pulgar lejos de los demás y no se encadena con la misma soltura.
const DOBLE_XBOX: Duration = Duration::from_millis(1000);

/// Lo que se espera **antes de enseñar nada**.
///
/// Sin esto, una pulsación normal y corriente enseñaba un destello del círculo de «mantén
/// para…» aunque no llegara a nada. Medio segundo basta para que una pulsación intencionada
/// no lo vea nunca, y sigue dejando margen de sobra para entender el gesto cuando sí se
/// mantiene. Es un patrón reutilizable para cualquier otro mantenido que se añada.
const GRACIA: Duration = Duration::from_millis(500);

/// Traduce el índice del *Standard Gamepad* del W3C —el que usa la interfaz— a la máscara de
/// XInput. Solo hacen falta los que tienen sentido para un gesto de mantener.
fn mascara(indice: u32) -> u16 {
    match indice {
        0 => 0x1000, // A
        1 => 0x2000, // B
        2 => 0x4000, // X
        3 => 0x8000, // Y
        4 => 0x0100, // LB
        5 => 0x0200, // RB
        8 => BOTON_BACK,
        9 => 0x0010, // Start
        10 => 0x0040, // L3
        11 => 0x0080, // R3
        _ => BOTON_BACK,
    }
}

/// Arranca el sondeo en un hilo propio. `al_gesto` se llama desde ese hilo.
pub fn escuchar(al_gesto: impl Fn(Gesto) + Send + 'static) {
    let Some(get_state) = cargar() else {
        return; // sin XInput no hay nada que escuchar; el resto de la app sigue igual
    };
    std::thread::spawn(move || {
        let mut slot: Option<u32> = None;
        let mut ultima_rebusca = Instant::now() - REBUSCA;
        let mut guia_antes = false;
        // Cuándo fue la pulsación anterior del botón Xbox, para la doble.
        let mut ultimo_xbox: Option<Instant> = None;
        let mut desde: Option<Instant> = None;
        let mut avisado_completo = false;
        // Cuándo acabó la pulsación corta anterior, para detectar la doble.
        let mut ultima_corta: Option<Instant> = None;

        loop {
            if !ESCUCHA.load(Ordering::Relaxed) {
                desde = None;
                std::thread::sleep(REBUSCA);
                continue;
            }
            // Buscar mando solo de vez en cuando: sondear huecos vacíos es caro.
            if slot.is_none() && ultima_rebusca.elapsed() >= REBUSCA {
                ultima_rebusca = Instant::now();
                for i in 0..4u32 {
                    let mut e = Estado::default();
                    if unsafe { get_state(i, &mut e) } == 0 {
                        slot = Some(i);
                        break;
                    }
                }
            }
            let Some(i) = slot else {
                std::thread::sleep(SONDEO);
                continue;
            };

            let mut e = Estado::default();
            if unsafe { get_state(i, &mut e) } != 0 {
                slot = None; // se ha desconectado
                desde = None;
                std::thread::sleep(SONDEO);
                continue;
            }
            let b = e.gamepad.botones;

            // ── Botón Xbox, por flanco de bajada ──────────────────────────────────────
            let guia = b & BOTON_GUIA != 0;
            if guia && !guia_antes && XBOX_TRAE.load(Ordering::Relaxed) {
                if !XBOX_DOBLE.load(Ordering::Relaxed) {
                    al_gesto(Gesto::Xbox);
                } else {
                    // Doble: la primera se guarda y solo dispara la segunda si llega a tiempo.
                    let encadena = ultimo_xbox.map(|u| u.elapsed() <= DOBLE_XBOX).unwrap_or(false);
                    if encadena {
                        ultimo_xbox = None;
                        al_gesto(Gesto::Xbox);
                    } else {
                        ultimo_xbox = Some(Instant::now());
                    }
                }
            }
            guia_antes = guia;

            // ── El botón del Modo Sofá ────────────────────────────────────────────────
            //
            // Este botón lo lleva **entero** el núcleo, incluida su pulsación corta. Si la
            // interfaz disparase su acción normal al pulsar, con el gesto de mantener se
            // abrirían las dos cosas a la vez.
            let sofa = b & mascara(BOTON_SOFA.load(Ordering::Relaxed)) != 0;
            let gesto = GESTO.load(Ordering::Relaxed);
            let tope = Duration::from_millis(
                MANTENER_MS.load(Ordering::Relaxed).max(GRACIA.as_millis() as u32 + 200) as u64,
            );

            match (sofa, desde) {
                (true, None) => {
                    // Nada de avisar al pulsar: hay que ver si de verdad se está manteniendo.
                    desde = Some(Instant::now());
                    avisado_completo = false;
                }
                (true, Some(t)) if gesto == 0 && !avisado_completo => {
                    let va = t.elapsed();
                    if va >= tope {
                        avisado_completo = true;
                        al_gesto(Gesto::Completado);
                    } else if va >= GRACIA {
                        // El círculo arranca vacío **al pasar la gracia**, no al pulsar: así
                        // una pulsación corta no llega a enseñarlo nunca.
                        let p = (va - GRACIA).as_secs_f32() / (tope - GRACIA).as_secs_f32();
                        al_gesto(Gesto::Progreso(p.clamp(0.0, 1.0)));
                    }
                }
                (false, Some(t)) => {
                    desde = None;
                    if avisado_completo {
                        // Soltar tras completarlo no cancela nada: la acción ya se disparó.
                    } else if gesto == 1 {
                        al_gesto(Gesto::Completado);
                    } else if gesto == 2 {
                        // Doble: la primera pulsación se guarda, la segunda dispara.
                        let encadena = ultima_corta
                            .map(|u| u.elapsed() <= DOBLE)
                            .unwrap_or(false);
                        if encadena {
                            ultima_corta = None;
                            al_gesto(Gesto::Completado);
                        } else {
                            ultima_corta = Some(Instant::now());
                            al_gesto(Gesto::Corta);
                        }
                        let _ = t;
                    } else {
                        // Mantener, pero se soltó antes: fue una pulsación corta y hay que
                        // decirlo, porque ese botón tiene otra acción asignada.
                        al_gesto(Gesto::Progreso(-1.0));
                        al_gesto(Gesto::Corta);
                    }
                }
                _ => {}
            }

            std::thread::sleep(SONDEO);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{mascara, BOTON_BACK};

    #[test]
    fn select_es_el_boton_por_defecto() {
        assert_eq!(mascara(8), BOTON_BACK);
        // Un índice que no se contempla cae en SELECT en vez de no responder a nada: es
        // preferible que el gesto funcione con el botón de fábrica a que deje de existir.
        assert_eq!(mascara(99), BOTON_BACK);
    }

    #[test]
    fn los_indices_del_w3c_se_traducen_a_xinput() {
        assert_eq!(mascara(0), 0x1000); // A
        assert_eq!(mascara(9), 0x0010); // Start
        assert_eq!(mascara(10), 0x0040); // L3
        assert_ne!(mascara(4), mascara(5)); // LB y RB no son el mismo
    }
}
