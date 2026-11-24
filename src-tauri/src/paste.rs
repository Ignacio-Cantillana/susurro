//! Pegar el texto en la app activa: se pone en el portapapeles, se simula Cmd+V
//! y se restaura lo que había antes.

use std::time::Duration;

use anyhow::{Result, anyhow};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KEY_V: u16 = 9;

pub fn paste_text(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text)?;
    std::thread::sleep(Duration::from_millis(40));
    send_cmd_v()?;
    // Hay apps que leen el portapapeles con algo de retraso.
    std::thread::sleep(Duration::from_millis(250));
    if let Some(prev) = previous {
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}

fn send_cmd_v() -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow!("no se pudo crear la fuente de eventos"))?;
    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(source.clone(), KEY_V, down)
            .map_err(|_| anyhow!("no se pudo crear el evento de teclado"))?;
        ev.set_flags(CGEventFlags::CGEventFlagCommand);
        ev.post(CGEventTapLocation::Session);
    }
    Ok(())
}

/// Nombre de la app que tiene el foco (para guardarlo con el dictado).
pub fn frontmost_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    let ws = NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    app.localizedName().map(|n| n.to_string())
}
