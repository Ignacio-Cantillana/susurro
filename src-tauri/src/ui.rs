//! Ventanas y menú: la botonera flotante (un NSPanel que no roba el foco), la
//! ventana principal y el icono de la barra de menús.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, StyleMask, tauri_panel};

use crate::state::AppState;

tauri_panel! {
    panel!(WidgetPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

pub const WIDGET: &str = "widget";
/// Minimizado: solo una pestañita pegada al borde.
const WIDGET_W_MINI: f64 = 14.0;
const WIDGET_W: f64 = 50.0;
const WIDGET_W_EXPANDED: f64 = 230.0;
const WIDGET_H: f64 = 180.0;
const MARGIN_LEFT: f64 = 2.0;

/// Posición del panel: pegado al borde izquierdo (Wispr Flow ocupa el derecho),
/// un poco por debajo del centro. Crece siempre hacia la derecha.
fn widget_frame(app: &AppHandle, width: f64) -> Option<(LogicalPosition<f64>, LogicalSize<f64>)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let pos = monitor.position().to_logical::<f64>(scale);
    let x = pos.x + MARGIN_LEFT;
    let y = pos.y + size.height * 0.58 - WIDGET_H / 2.0;
    Some((LogicalPosition::new(x, y), LogicalSize::new(width, WIDGET_H)))
}

pub fn create_widget(app: &AppHandle) -> tauri::Result<()> {
    let (pos, size) = widget_frame(app, WIDGET_W_MINI).unwrap_or((LogicalPosition::new(100.0, 100.0), LogicalSize::new(WIDGET_W_MINI, WIDGET_H)));
    let panel = PanelBuilder::<_, WidgetPanel>::new(app, WIDGET)
        .url(WebviewUrl::App("widget.html".into()))
        .position(tauri::Position::Logical(pos))
        .size(tauri::Size::Logical(size))
        .level(PanelLevel::Floating)
        .floating(true)
        .has_shadow(false)
        .opaque(false)
        .hides_on_deactivate(false)
        .becomes_key_only_if_needed(true)
        .no_activate(true)
        .style_mask(StyleMask::empty().nonactivating_panel().borderless())
        .collection_behavior(
            CollectionBehavior::new().can_join_all_spaces().stationary().full_screen_auxiliary().ignores_cycle(),
        )
        .with_window(|w| w.decorations(false).transparent(true).shadow(false).resizable(false).skip_taskbar(true))
        .build()?;
    panel.set_transparent(true);
    let visible = app.state::<Arc<AppState>>().settings.read().show_widget;
    if visible {
        panel.show();
    }
    Ok(())
}

/// Dónde está el ratón respecto a la botonera. El panel no coge el foco, así que la web
/// no recibe bien `mouseenter`/`mouseleave` ni `:hover`; lo miramos desde aquí y le
/// mandamos la posición en coordenadas del panel para que sepa qué botón hay debajo.
#[derive(Clone, Copy, PartialEq, serde::Serialize)]
struct WidgetHover {
    inside: bool,
    x: f64,
    y: f64,
}

pub fn spawn_hover_watch(app: &AppHandle) {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let app = app.clone();
    let _ = std::thread::Builder::new().name("widget-hover".into()).spawn(move || {
        let mut last = WidgetHover { inside: false, x: -1.0, y: -1.0 };
        loop {
            // Más a menudo con el ratón encima, para que el resaltado vaya fino.
            std::thread::sleep(std::time::Duration::from_millis(if last.inside { 40 } else { 90 }));
            let Some(w) = app.get_webview_window(WIDGET) else { continue };
            if !w.is_visible().unwrap_or(false) {
                continue;
            }
            let (Ok(pos), Ok(size), Ok(scale)) = (w.outer_position(), w.outer_size(), w.scale_factor()) else { continue };
            let (pos, size) = (pos.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
            let Some(mouse) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                .ok()
                .and_then(|s| CGEvent::new(s).ok())
                .map(|e| e.location())
            else {
                continue;
            };
            // Hasta el borde de la pantalla por la izquierda: así basta con llevar el ratón al borde.
            let inside = mouse.x <= pos.x + size.width
                && mouse.x >= pos.x - MARGIN_LEFT - 2.0
                && mouse.y >= pos.y
                && mouse.y <= pos.y + size.height;
            let now = if inside {
                WidgetHover { inside, x: (mouse.x - pos.x).round(), y: (mouse.y - pos.y).round() }
            } else {
                WidgetHover { inside, x: -1.0, y: -1.0 }
            };
            if now != last {
                last = now;
                let _ = w.emit("widget-hover", now);
            }
        }
    });
}

pub fn set_widget_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window(WIDGET) {
        let _ = if visible { w.show() } else { w.hide() };
    }
}

/// Tamaño del panel: `mini` (pestañita), `normal` (botones) o `label` (botones y
/// etiqueta a la derecha, para «Dictar fn» o un aviso).
pub fn widget_expand(app: &AppHandle, mode: &str) {
    let width = match mode {
        "mini" => WIDGET_W_MINI,
        "label" => WIDGET_W_EXPANDED,
        _ => WIDGET_W,
    };
    if let (Some(w), Some((pos, size))) = (app.get_webview_window(WIDGET), widget_frame(app, width)) {
        let _ = w.set_size(size);
        let _ = w.set_position(pos);
    }
}

/// Con la ventana abierta, Susurro sale en el Dock y en ⌘Tab; al cerrarla vuelve a
/// vivir solo en la barra de menús y en los botones flotantes.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = w.emit("navigate", p);
        }
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Susurro", true, None::<&str>)?;
    let meeting = MenuItem::with_id(app, "meeting", "Empezar/parar reunión", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "widget", "Mostrar/ocultar botones flotantes", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &meeting, &widget, &PredefinedMenuItem::separator(app)?, &quit])?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Susurro")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, ev| {
            let st = app.state::<Arc<AppState>>().inner().clone();
            match ev.id().as_ref() {
                "open" => show_main(app, None),
                "meeting" => {
                    if st.meeting.lock().is_some() {
                        crate::meeting::stop(&st);
                    } else if let Err(e) = crate::meeting::start(&st, None) {
                        st.error(format!("{e:#}"));
                    }
                }
                "widget" => {
                    let visible = {
                        let mut s = st.settings.write();
                        s.show_widget = !s.show_widget;
                        let _ = s.save(&st.db.lock());
                        s.show_widget
                    };
                    set_widget_visible(app, visible);
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}
