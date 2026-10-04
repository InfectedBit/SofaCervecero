//! Control del **cursor del sistema** desde el mando (guión E.0_3).
//!
//! El stick derecho mueve el ratón y un gatillo hace clic, para llegar con el mando a lo que
//! aún no es navegable por foco. Mover el puntero real **no** se puede hacer desde el WebView:
//! es una acción del sistema operativo, así que vive aquí.
//!
//! El estado (posición, botón pulsado) lo lleva Windows; este módulo es sin estado a
//! propósito: si el hub se cierra a media pulsación, se suelta el botón al salir.

/// Mueve el cursor `dx`/`dy` píxeles **en relativo** a donde esté ahora.
pub fn mover(dx: i32, dy: i32) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
        unsafe {
            let mut p = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut p) != 0 {
                SetCursorPos(p.x + dx, p.y + dy);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (dx, dy);
    }
}

/// Pulsa o suelta un botón del ratón. Se exponen los dos flancos (y no un "clic" entero)
/// para que se pueda **arrastrar**: mantener el gatillo equivale a mantener pulsado.
pub fn boton(izquierdo: bool, pulsado: bool) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
        };
        let flags = match (izquierdo, pulsado) {
            (true, true) => MOUSEEVENTF_LEFTDOWN,
            (true, false) => MOUSEEVENTF_LEFTUP,
            (false, true) => MOUSEEVENTF_RIGHTDOWN,
            (false, false) => MOUSEEVENTF_RIGHTUP,
        };
        let entrada = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(1, &entrada, std::mem::size_of::<INPUT>() as i32);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (izquierdo, pulsado);
    }
}
