//! Atajo global con un CGEventTap: detecta la tecla modificadora elegida (Fn por
//! defecto) mantenida, Fn+Espacio para el modo manos libres y Esc para cancelar.
//!
//! Necesita los permisos de Monitorización de entrada y Accesibilidad. Si no están
//! concedidos, reintenta cada 2 s hasta que el usuario los dé.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, Ordering};
use std::sync::mpsc::Sender;

use core_foundation::base::TCFType;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use parking_lot::Mutex;

use crate::settings::Hotkey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Se ha pulsado la tecla de dictado.
    Down,
    /// Se ha soltado la tecla de dictado.
    Up,
    /// Tecla de dictado + Espacio: cambiar a manos libres.
    Lock,
    /// Se pulsó otra tecla con la de dictado (Fn+F1, Fn+flecha…): no era un dictado.
    Interrupted,
    /// Esc mientras se graba.
    Cancel,
}

const KEY_FN: i64 = 63;
const KEY_SPACE: i64 = 49;
const KEY_ESCAPE: i64 = 53;

static HOTKEY: AtomicU8 = AtomicU8::new(0);
/// El controlador lo pone a `true` mientras graba, para que Esc y Espacio se consuman.
pub static RECORDING: AtomicBool = AtomicBool::new(false);
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static TAP_OK: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn CGEventTapEnable(tap: *const c_void, enable: bool);
}

pub fn set_hotkey(h: Hotkey) {
    HOTKEY.store(h as u8, Ordering::Relaxed);
}

/// ¿Está el atajo funcionando? (si no, faltan permisos).
pub fn is_active() -> bool {
    TAP_OK.load(Ordering::Relaxed)
}

/// (keycode, máscara de flag) de la tecla configurada.
fn target() -> (i64, CGEventFlags) {
    match HOTKEY.load(Ordering::Relaxed) {
        x if x == Hotkey::RightOption as u8 => (61, CGEventFlags::CGEventFlagAlternate),
        x if x == Hotkey::RightCommand as u8 => (54, CGEventFlags::CGEventFlagCommand),
        _ => (KEY_FN, CGEventFlags::CGEventFlagSecondaryFn),
    }
}

pub fn spawn(tx: Sender<HotkeyEvent>) {
    std::thread::Builder::new()
        .name("hotkey".into())
        .spawn(move || {
            loop {
                match run_tap(tx.clone()) {
                    Ok(()) => return,
                    Err(()) => {
                        TAP_OK.store(false, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
            }
        })
        .expect("hilo del atajo");
}

fn run_tap(tx: Sender<HotkeyEvent>) -> Result<(), ()> {
    // Estado del gesto, solo lo toca el callback (siempre en este hilo).
    let held = Mutex::new(false);
    let interrupted = Mutex::new(false);

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
        move |_proxy, etype, event| {
            match etype {
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                    let port = TAP_PORT.load(Ordering::Relaxed);
                    if !port.is_null() {
                        unsafe { CGEventTapEnable(port, true) };
                    }
                }
                CGEventType::FlagsChanged => {
                    let (code, mask) = target();
                    if event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) == code {
                        let down = event.get_flags().contains(mask);
                        let mut h = held.lock();
                        if down && !*h {
                            *h = true;
                            *interrupted.lock() = false;
                            let _ = tx.send(HotkeyEvent::Down);
                        } else if !down && *h {
                            *h = false;
                            if !*interrupted.lock() {
                                let _ = tx.send(HotkeyEvent::Up);
                            }
                        }
                        // Fn nos lo quedamos, como Wispr Flow: si no, otras apps con dictado
                        // propio (Warp…) también lo ven y el texto se pega dos veces.
                        // Fn+flecha y compañía siguen funcionando: el flag va en cada tecla.
                        // ⌥ y ⌘ sí se dejan pasar, porque las apps llevan la cuenta de
                        // esos modificadores y se quedarían «pegados».
                        if code == KEY_FN {
                            return CallbackResult::Drop;
                        }
                    } else if *held.lock() {
                        // Otro modificador con la tecla de dictado pulsada (Fn+Cmd…).
                        *interrupted.lock() = true;
                        let _ = tx.send(HotkeyEvent::Interrupted);
                    }
                }
                CGEventType::KeyDown => {
                    let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    let recording = RECORDING.load(Ordering::Relaxed);
                    if *held.lock() {
                        if code == KEY_SPACE {
                            // Fn+Espacio: manos libres. El Fn que se suelta después no para.
                            *interrupted.lock() = true;
                            let _ = tx.send(HotkeyEvent::Lock);
                            return CallbackResult::Drop;
                        }
                        *interrupted.lock() = true;
                        let _ = tx.send(HotkeyEvent::Interrupted);
                    } else if recording && code == KEY_ESCAPE {
                        let _ = tx.send(HotkeyEvent::Cancel);
                        return CallbackResult::Drop;
                    }
                }
                _ => {}
            }
            CallbackResult::Keep
        },
    )?;

    TAP_PORT.store(tap.mach_port().as_concrete_TypeRef() as *mut c_void, Ordering::Relaxed);
    let source = tap.mach_port().create_runloop_source(0).map_err(|_| ())?;
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    TAP_OK.store(true, Ordering::Relaxed);
    log::info!("atajo global activo");
    CFRunLoop::run_current();
    Ok(())
}
