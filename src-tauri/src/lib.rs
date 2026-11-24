//! Susurro: dictado por voz y transcripción de reuniones, en local.

mod audio;
mod commands;
mod dictation;
mod hotkey;
mod meeting;
mod models;
mod paste;
mod permissions;
mod secrets;
mod settings;
mod state;
mod transcribe;
mod ui;

use std::sync::Arc;

use susurro_core::Db;
use tauri::{Manager, WindowEvent};

use crate::state::AppState;

/// Modo línea de comandos: `susurro --transcribe audio.wav [--model id] [--lang es]`.
/// Transcribe con el modelo local y sale. Útil para probar y medir sin la interfaz.
pub fn cli(args: &[String]) -> Option<i32> {
    let pos = args.iter().position(|a| a == "--transcribe")?;
    env_logger_lite();
    let arg = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let path = args.get(pos + 1)?;
    let model = models::find(&arg("--model").unwrap_or_else(|| models::CATALOG[0].id.into()))?;
    let lang = arg("--lang").unwrap_or_else(|| "es".into());
    let run = || -> anyhow::Result<()> {
        let audio = susurro_core::audio::decode_wav(&std::fs::read(path)?)
            .ok_or_else(|| anyhow::anyhow!("solo WAV PCM de 16 bits"))?;
        let engine = transcribe::LocalTranscriber::new(models::download(model, |d, t| {
            eprint!("\rdescargando {}: {} %", model.name, d * 100 / t.max(1));
        })?);
        let t = std::time::Instant::now();
        engine.warm_up()?;
        eprintln!("modelo cargado en {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let req = transcribe::Request { language: (lang != "auto").then_some(lang.as_str()), prompt: None };
        let pieces = transcribe::Transcriber::transcribe(&engine, &audio, &req)?;
        eprintln!(
            "{:.1} s de audio transcritos en {:?}",
            audio.len() as f32 / susurro_core::audio::WHISPER_RATE as f32,
            t.elapsed()
        );
        println!("{}", transcribe::join(&pieces));
        Ok(())
    };
    Some(match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger_lite();
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // App de barra de menús: sin icono en el Dock.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let db = Db::open(&susurro_core::default_db_path())?;
            let st = Arc::new(AppState::new(app.handle().clone(), db));
            app.manage(st.clone());

            let (tx, rx) = std::sync::mpsc::channel();
            hotkey::spawn(tx);
            dictation::run_hotkey_loop(app.handle().clone(), rx);

            ui::create_tray(app.handle())?;
            ui::create_widget(app.handle())?;
            ui::spawn_hover_watch(app.handle());

            if !st.settings.read().onboarding_done {
                ui::show_main(app.handle(), Some("onboarding"));
            }

            // Los dictados caducan (24 h por defecto): limpieza al arrancar y cada hora.
            let janitor = st.clone();
            std::thread::Builder::new().name("limpieza".into()).spawn(move || loop {
                janitor.purge_old_dictations();
                std::thread::sleep(std::time::Duration::from_secs(3600));
            })?;

            // Carga el modelo en segundo plano para que el primer dictado sea rápido.
            let warm = st.clone();
            std::thread::spawn(move || {
                if warm.settings.read().provider == settings::Provider::Local {
                    if let Err(e) = warm.local.warm_up() {
                        log::info!("modelo no cargado todavía: {e:#}");
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Cerrar la ventana principal la oculta; la app sigue en la barra de menús.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    ui::hide_main(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::toggle_dictation,
            commands::cancel_dictation,
            commands::start_meeting,
            commands::stop_meeting,
            commands::list_entries,
            commands::update_entry,
            commands::delete_entry,
            commands::search,
            commands::copy_text,
            commands::list_meetings,
            commands::get_meeting,
            commands::meeting_markdown,
            commands::rename_meeting,
            commands::delete_meeting,
            commands::list_dictionary,
            commands::upsert_dictionary,
            commands::delete_dictionary,
            commands::list_snippets,
            commands::upsert_snippet,
            commands::delete_snippet,
            commands::get_settings,
            commands::save_settings,
            commands::list_input_devices,
            commands::set_api_key,
            commands::has_api_key,
            commands::check_permissions,
            commands::request_permission,
            commands::list_models,
            commands::download_model,
            commands::delete_model,
            commands::show_main,
            commands::widget_expand,
            commands::mcp_info,
        ])
        .build(tauri::generate_context!())
        .expect("error al arrancar Susurro")
        .run(|app, event| {
            // Abrir Susurro otra vez (Spotlight, Raycast, Aplicaciones…) enseña la ventana.
            if let tauri::RunEvent::Reopen { .. } = event {
                ui::show_main(app, None);
            }
        });
}

/// Logs a stderr con nivel configurable por `RUST_LOG` (info por defecto), sin dependencias.
fn env_logger_lite() {
    struct Logger(log::LevelFilter);
    impl log::Log for Logger {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= self.0 && m.target().starts_with("susurro")
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("[{}] {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let level = std::env::var("RUST_LOG").ok().and_then(|l| l.parse().ok()).unwrap_or(log::LevelFilter::Info);
    let logger: &'static Logger = Box::leak(Box::new(Logger(level)));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}
