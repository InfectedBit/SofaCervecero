//! Servicio residente: bandeja del sistema + ciclo de vida "core residente + UI bajo demanda".
//! Ver plan §5.1. Cerrar la ventana la **destruye** (libera WebView2, ~165 MB privados); el core
//! Rust (~28 MB) queda residente en la bandeja. Reabrir crea una ventana nueva.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// Crea el icono de bandeja con menú (Abrir hub / Salir) y clic para abrir.
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Abrir hub", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("SofaCervecero")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_hub(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_hub(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Clave donde se guarda la geometría de la ventana (guión H.0_2).
const GEOMETRIA: &str = "ventana.geometria";

/// Tamaño y posición de la ventana, serializados como `ancho,alto,x,y`.
///
/// Se guardan a mano y no con el comportamiento por defecto de Tauri porque la ventana **se
/// destruye al cerrarla** (para liberar el WebView2, plan §5.1): cada vez que se abre el hub
/// se construye una nueva, así que hay que aplicarle la geometría explícitamente.
fn leer_geometria(app: &AppHandle) -> Option<(f64, f64, i32, i32)> {
    let estado = app.state::<crate::AppState>();
    let conn = estado.db.lock().ok()?;
    let v = crate::library::get_setting(&conn, GEOMETRIA).ok()??;
    let p: Vec<&str> = v.split(',').collect();
    if p.len() != 4 {
        return None;
    }
    Some((
        p[0].parse().ok()?,
        p[1].parse().ok()?,
        p[2].parse().ok()?,
        p[3].parse().ok()?,
    ))
}

/// ¿La esquina superior izquierda guardada cae dentro de algún monitor conectado ahora?
///
/// Se deja un margen por arriba: basta con que se vea la barra de título para poder mover la
/// ventana a mano. Si no hay forma de consultar los monitores, se da por buena la posición —
/// es lo que se hacía antes y no empeora nada.
fn en_alguna_pantalla(app: &AppHandle, x: i32, y: i32) -> bool {
    let Ok(monitores) = app.available_monitors() else {
        return true;
    };
    if monitores.is_empty() {
        return true;
    }
    monitores.iter().any(|m| {
        let p = m.position();
        let t = m.size();
        x >= p.x
            && x < p.x + t.width as i32
            && y >= p.y
            && y < p.y + t.height as i32
    })
}

fn guardar_geometria(app: &AppHandle, w: &tauri::WebviewWindow) {
    // Una ventana minimizada, maximizada o **a pantalla completa** reporta medidas que no sirven
    // para restaurar. Lo de la pantalla completa importa desde el Modo Sofá (E.0_4): cerrar el
    // hub desde el televisor guardaría 4096×2160 en (−4096,−706) como «la ventana de siempre».
    if w.is_minimized().unwrap_or(false)
        || w.is_maximized().unwrap_or(false)
        || w.is_fullscreen().unwrap_or(false)
    {
        return;
    }
    let (Ok(tam), Ok(pos)) = (w.inner_size(), w.outer_position()) else {
        return;
    };
    let escala = w.scale_factor().unwrap_or(1.0);
    let logico = tam.to_logical::<f64>(escala);
    // Tamaños absurdos (ventana colapsada) no se guardan: dejarían el hub inusable.
    if logico.width < 400.0 || logico.height < 300.0 {
        return;
    }
    let valor = format!("{},{},{},{}", logico.width, logico.height, pos.x, pos.y);
    let estado = app.state::<crate::AppState>();
    if let Ok(conn) = estado.db.lock() {
        let _ = crate::library::set_setting(&conn, GEOMETRIA, &valor);
    }
}

/// Trae la ventana al frente **de verdad**, aunque esté detrás de un juego a pantalla completa.
///
/// `set_focus` no basta: Windows **bloquea** que un proceso en segundo plano robe el primer plano
/// —si no, cualquier programa podría saltar encima de lo que estés haciendo—. La forma admitida
/// de pedirlo es engancharse a la cola de entrada del hilo que **sí** está en primer plano: con
/// eso el sistema considera que la petición viene de quien tiene el foco y la concede.
///
/// Hace falta para el botón Xbox del Modo Sofá (guión E.0_4 §9): ahí la ventana puede estar
/// minimizada, detrás de un juego, o en otro monitor.
#[cfg(windows)]
fn traer_al_frente(w: &tauri::WebviewWindow) {
    let Ok(hwnd) = w.hwnd() else { return };
    enfocar(hwnd.0 as _);
}

/// La ventana que tenía el primer plano cuando el hub se lo quitó. 0 = ninguna.
///
/// Es el camino de vuelta, y tiene que existir **dentro de la app**: con el hub a pantalla
/// completa en el televisor, traerlo al frente tapa el juego por completo. Que Windows ofrezca su
/// propia tabulación de ventanas al mantener el botón Xbox es cierto hoy y **no se puede dar por
/// hecho**: depende de que la Game Bar esté activada, de que su atajo de mando lo esté, y muchos
/// juegos a pantalla completa exclusiva se comen el overlay. Si eso falla y nosotros no tenemos
/// vuelta, el juego queda inalcanzable.
#[cfg(windows)]
static ANTERIOR: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Pone `hwnd` en primer plano saltándose el bloqueo de Windows.
#[cfg(windows)]
fn enfocar(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        ShowWindow, SW_RESTORE,
    };
    unsafe {
        let actual = GetForegroundWindow();
        let hilo_actual = GetWindowThreadProcessId(actual, std::ptr::null_mut());
        let hilo_propio = GetCurrentThreadId();

        // Engancharse solo si son hilos distintos: adjuntarse a sí mismo no está definido.
        let enganchar = hilo_actual != 0 && hilo_actual != hilo_propio;
        if enganchar {
            AttachThreadInput(hilo_propio, hilo_actual, 1);
        }
        ShowWindow(hwnd, SW_RESTORE);
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        if enganchar {
            AttachThreadInput(hilo_propio, hilo_actual, 0);
        }
    }
}

/// Apunta quién tiene el primer plano **antes** de que el hub se lo quite.
#[cfg(windows)]
fn apuntar_anterior(app: &AppHandle) {
    use std::sync::atomic::Ordering::Relaxed;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let actual = unsafe { GetForegroundWindow() };
    if actual.is_null() {
        return;
    }
    // La propia ventana del hub no es un sitio al que volver.
    let propia = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize);
    if propia == Some(actual as isize) {
        return;
    }
    ANTERIOR.store(actual as isize, Relaxed);
}

/// Devuelve el primer plano a la ventana que lo tenía antes. `false` si ya no hay a dónde volver.
///
/// Es lo que hace del botón Xbox un **interruptor** en vez de un camino de ida: la misma pulsación
/// que trae el hub lo aparta. Si la ventana apuntada ya no existe —el juego se cerró— no se
/// inventa nada y el gesto pasa a significar otra cosa.
#[cfg(windows)]
pub fn volver_al_anterior() -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible};

    let apuntada = ANTERIOR.load(Relaxed);
    if apuntada == 0 {
        return false;
    }
    let hwnd = apuntada as windows_sys::Win32::Foundation::HWND;
    if unsafe { IsWindow(hwnd) } == 0 || unsafe { IsWindowVisible(hwnd) } == 0 {
        ANTERIOR.store(0, Relaxed);
        return false;
    }
    enfocar(hwnd);
    // Se gasta al usarla: si el juego se cierra después, el botón no intenta volver a una ventana
    // muerta, y vuelve a significar «trae el hub».
    ANTERIOR.store(0, Relaxed);
    true
}

/// ¿Es la ventana del hub la que está en primer plano **ahora mismo**?
///
/// Distingue los dos casos que antes se confundían: «el hub está abierto» y «el hub está
/// delante». Tener la ventana creada pero detrás de un juego no es tenerla a mano.
#[cfg(windows)]
pub fn es_primer_plano(app: &AppHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let Some(w) = app.get_webview_window("main") else {
        return false;
    };
    let Ok(hwnd) = w.hwnd() else { return false };
    unsafe { GetForegroundWindow() == hwnd.0 as _ }
}

/// Muestra el hub: reenfoca la ventana si existe, o crea una nueva (materialización on-demand).
pub fn show_hub(app: &AppHandle) {
    // Antes de robarle el primer plano a nadie, apuntar a quién: es el camino de vuelta.
    #[cfg(windows)]
    apuntar_anterior(app);

    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        #[cfg(windows)]
        traer_al_frente(&w);
        return;
    }

    let (ancho, alto, x, y) = leer_geometria(app).unwrap_or((1100.0, 720.0, 0, 0));
    let mut b = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("SofaCervecero")
        .inner_size(ancho, alto);
    // Sin posición guardada (primer arranque), que la coloque el sistema. Y si la guardada
    // cae fuera de todas las pantallas —el monitor de entonces ya no está— se descarta: una
    // ventana colocada en un escritorio que no existe **no se ve**, y la app parece no abrir.
    if (x, y) != (0, 0) && en_alguna_pantalla(app, x, y) {
        b = b.position(x as f64, y as f64);
    }
    if let Ok(w) = b.build() {
        // Se guarda al cerrar, que es justo antes de que la ventana deje de existir.
        let app2 = app.clone();
        let w2 = w.clone();
        w.on_window_event(move |e| {
            if matches!(e, tauri::WindowEvent::CloseRequested { .. }) {
                guardar_geometria(&app2, &w2);
            }
        });
    }
}
