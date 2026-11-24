//! Estado de los permisos de macOS que necesita Susurro.

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use serde::Serialize;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

/// Respuesta de `TCCAccessPreflight`: 0 = concedido, 1 = denegado, 2 = sin preguntar.
const TCC_GRANTED: i32 = 0;
const TCC_NOT_ASKED: i32 = 2;

/// ¿Puede Susurro grabar el audio del sistema? No hay API pública para consultarlo
/// sin lanzar el diálogo, así que usamos la de TCC (privada, la misma que usa la
/// app de ejemplo AudioCap) cargada en tiempo de ejecución. Si no está, «no».
fn system_audio_granted() -> bool {
    system_audio_preflight() == Some(TCC_GRANTED)
}

fn system_audio_preflight() -> Option<i32> {
    type Preflight = unsafe extern "C" fn(CFStringRef, CFDictionaryRef) -> i32;
    let sym = tcc_symbol(c"TCCAccessPreflight")?;
    let f = unsafe { std::mem::transmute::<*mut libc::c_void, Preflight>(sym) };
    let service = CFString::from_static_string(TCC_AUDIO_CAPTURE);
    Some(unsafe { f(service.as_concrete_TypeRef(), std::ptr::null()) })
}

/// Saca el diálogo de macOS para el audio del sistema. Crear un tap no basta: macOS
/// lo deja crear sin permiso (y solo da silencio), así que se pide a TCC directamente.
fn request_system_audio() {
    type Request = unsafe extern "C" fn(CFStringRef, CFDictionaryRef, *mut libc::c_void);
    let Some(sym) = tcc_symbol(c"TCCAccessRequest") else { return };
    let f = unsafe { std::mem::transmute::<*mut libc::c_void, Request>(sym) };
    let done = block2::RcBlock::new(|_granted: objc2::runtime::Bool| {});
    let service = CFString::from_static_string(TCC_AUDIO_CAPTURE);
    unsafe { f(service.as_concrete_TypeRef(), std::ptr::null(), &*done as *const _ as *mut libc::c_void) };
}

const TCC_AUDIO_CAPTURE: &str = "kTCCServiceAudioCapture";

fn tcc_symbol(name: &std::ffi::CStr) -> Option<*mut libc::c_void> {
    struct Lib(*mut libc::c_void);
    unsafe impl Send for Lib {}
    unsafe impl Sync for Lib {}
    static LIB: std::sync::OnceLock<Lib> = std::sync::OnceLock::new();
    let lib = LIB.get_or_init(|| unsafe {
        Lib(libc::dlopen(c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC".as_ptr(), libc::RTLD_NOW))
    });
    if lib.0.is_null() {
        return None;
    }
    let sym = unsafe { libc::dlsym(lib.0, name.as_ptr()) };
    (!sym.is_null()).then_some(sym)
}

#[derive(Debug, Clone, Serialize)]
pub struct Permissions {
    pub accessibility: bool,
    pub input_monitoring: bool,
    /// «Solo grabación del audio del sistema», para las reuniones.
    pub system_audio: bool,
    /// El atajo global está funcionando.
    pub hotkey_active: bool,
}

pub fn check() -> Permissions {
    unsafe {
        Permissions {
            accessibility: AXIsProcessTrusted(),
            input_monitoring: CGPreflightListenEventAccess(),
            system_audio: system_audio_granted(),
            hotkey_active: crate::hotkey::is_active(),
        }
    }
}

/// Lanza el diálogo del sistema (si procede) y abre el panel de Ajustes correspondiente.
pub fn request(kind: &str) {
    let pane = match kind {
        "accessibility" => {
            // Muestra el diálogo del sistema y añade Susurro a la lista de Accesibilidad.
            let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
            let opts = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
            if unsafe { AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) } {
                return;
            }
            "Privacy_Accessibility"
        }
        "input_monitoring" => {
            unsafe { CGRequestListenEventAccess() };
            "Privacy_ListenEvent"
        }
        "system_audio" => {
            match system_audio_preflight() {
                Some(TCC_GRANTED) => return,
                // Sin preguntar todavía: basta el diálogo del sistema.
                Some(TCC_NOT_ASKED) => return request_system_audio(),
                // Denegado (o no sabemos): solo queda activarlo en Ajustes.
                _ => "Privacy_AudioCapture",
            }
        }
        "microphone" => "Privacy_Microphone",
        _ => return,
    };
    let _ = std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn();
}
