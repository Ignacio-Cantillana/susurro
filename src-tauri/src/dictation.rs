//! Máquina de estados del dictado y de las notas de voz.
//!
//! Inactivo → (Fn abajo) → Grabando → (Fn arriba) → Transcribiendo → Inactivo
//!                            │  └─ Fn+Espacio → Grabando (manos libres) → Fn → Transcribiendo
//!                            └─ Esc / otra tecla → cancelado

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use susurro_core::audio::{WHISPER_RATE, has_speech};
use susurro_core::db::{EntryKind, NewEntry};
use susurro_core::textproc;
use tauri::Manager;

use crate::audio::mic::MicCapture;
use crate::hotkey::{self, HotkeyEvent};
use crate::paste;
use crate::state::AppState;
use crate::transcribe;

/// Pulsaciones más cortas que esto se ignoran (un toque accidental de Fn).
const MIN_HOLD: Duration = Duration::from_millis(250);
/// Doble toque de Fn: si el segundo llega antes de esto, el dictado queda en manos
/// libres hasta la siguiente pulsación.
const DOUBLE_TAP: Duration = Duration::from_millis(450);

/// Cuándo acabó el último toque corto de Fn, para detectar el doble toque.
static LAST_TAP: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dictation,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub phase: Phase,
    pub mode: Option<Mode>,
    pub locked: bool,
}

struct Recording {
    mic: MicCapture,
    mode: Mode,
    locked: bool,
    started: Instant,
    app: Option<String>,
}

#[derive(Default)]
pub struct Dictation {
    recording: Option<Recording>,
    transcribing: bool,
}

impl Dictation {
    pub fn status(&self) -> Status {
        match &self.recording {
            Some(r) => Status { phase: Phase::Recording, mode: Some(r.mode), locked: r.locked },
            None if self.transcribing => Status { phase: Phase::Transcribing, mode: None, locked: false },
            None => Status { phase: Phase::Idle, mode: None, locked: false },
        }
    }
}

fn state(app: &tauri::AppHandle) -> tauri::State<'_, Arc<AppState>> {
    app.state::<Arc<AppState>>()
}

/// Bucle que recibe los eventos del atajo global.
pub fn run_hotkey_loop(app: tauri::AppHandle, rx: std::sync::mpsc::Receiver<HotkeyEvent>) {
    std::thread::Builder::new()
        .name("dictation".into())
        .spawn(move || {
            for ev in rx {
                let st = state(&app).inner().clone();
                handle(&st, ev);
            }
        })
        .expect("hilo de dictado");
}

fn handle(st: &Arc<AppState>, ev: HotkeyEvent) {
    let (recording, locked, held_for) = {
        let d = st.dictation.lock();
        match &d.recording {
            Some(r) => (true, r.locked, r.started.elapsed()),
            None => (false, false, Duration::ZERO),
        }
    };
    match ev {
        HotkeyEvent::Down if !recording => {
            let double = LAST_TAP.lock().take().is_some_and(|t| t.elapsed() < DOUBLE_TAP);
            start(st, Mode::Dictation, double);
        }
        HotkeyEvent::Down if locked => finish(st),
        HotkeyEvent::Up if recording && !locked => {
            if held_for < MIN_HOLD {
                *LAST_TAP.lock() = Some(Instant::now());
                cancel(st, false)
            } else {
                finish(st)
            }
        }
        HotkeyEvent::Lock if recording => {
            if let Some(r) = st.dictation.lock().recording.as_mut() {
                r.locked = true;
            }
            publish(st);
        }
        HotkeyEvent::Interrupted if recording && !locked => cancel(st, false),
        HotkeyEvent::Cancel if recording => cancel(st, true),
        _ => {}
    }
}

/// Botón del widget: empieza en manos libres o, si ya graba en ese modo, termina.
pub fn toggle(st: &Arc<AppState>, mode: Mode) {
    let current = st.dictation.lock().recording.as_ref().map(|r| r.mode);
    match current {
        None => start(st, mode, true),
        Some(m) if m == mode => finish(st),
        Some(_) => {}
    }
}

pub fn start(st: &Arc<AppState>, mode: Mode, locked: bool) {
    if st.dictation.lock().transcribing && st.settings.read().provider == crate::settings::Provider::Local {
        // Whisper local va de uno en uno; se encola sin problema, pero avisamos.
        log::info!("grabando mientras se transcribe el anterior");
    }
    let device = st.settings.read().input_device.clone();
    let mic = match MicCapture::start(device.as_deref()) {
        Ok(m) => m,
        Err(e) => return st.error(format!("{e:#}")),
    };
    let app = if mode == Mode::Dictation { paste::frontmost_app() } else { None };
    st.dictation.lock().recording = Some(Recording { mic, mode, locked, started: Instant::now(), app });
    hotkey::RECORDING.store(true, std::sync::atomic::Ordering::Relaxed);
    st.sound("Morse");
    publish(st);
    spawn_level_meter(st.clone());
}

fn take_recording(st: &AppState) -> Option<Recording> {
    let r = st.dictation.lock().recording.take();
    hotkey::RECORDING.store(false, std::sync::atomic::Ordering::Relaxed);
    r
}

pub fn cancel(st: &Arc<AppState>, audible: bool) {
    if let Some(r) = take_recording(st) {
        drop(r.mic.stop());
        if audible {
            st.sound("Funk");
        }
    }
    publish(st);
}

fn finish(st: &Arc<AppState>) {
    let Some(rec) = take_recording(st) else { return };
    st.sound("Pop");
    let duration_ms = rec.started.elapsed().as_millis() as i64;
    let audio = rec.mic.stop();
    st.dictation.lock().transcribing = true;
    publish(st);

    let st = st.clone();
    std::thread::spawn(move || {
        let result = process(&st, &audio, rec.mode, rec.app, duration_ms);
        st.dictation.lock().transcribing = false;
        publish(&st);
        if let Err(e) = result {
            st.error(format!("{e:#}"));
        }
    });
}

fn process(st: &AppState, audio: &[f32], mode: Mode, app: Option<String>, duration_ms: i64) -> anyhow::Result<()> {
    if !has_speech(audio, WHISPER_RATE) {
        st.emit("dictation-empty", ());
        return Ok(());
    }
    let job = st.job()?;
    let t = Instant::now();
    let pieces = job.engine.transcribe(audio, &job.request())?;
    let raw = transcribe::join(&pieces);
    log::info!("transcrito {:.1}s de audio en {:?}", audio.len() as f32 / WHISPER_RATE as f32, t.elapsed());
    let Some(text) = textproc::process(&raw, &job.dict, &job.snippets, &job.opts) else {
        st.emit("dictation-empty", ());
        return Ok(());
    };
    if mode == Mode::Dictation {
        paste::paste_text(&text)?;
    }
    let kind = if mode == Mode::Dictation { EntryKind::Dictation } else { EntryKind::Note };
    let id = st.db.lock().insert_entry(&NewEntry { kind, text: text.clone(), raw_text: raw, app, duration_ms })?;
    st.emit("entries-changed", serde_json::json!({ "id": id, "kind": kind, "text": text }));
    Ok(())
}

pub fn publish(st: &AppState) {
    let status = st.dictation.lock().status();
    st.emit("dictation-status", status);
}

/// Mientras se graba, manda el nivel del micro ~20 veces por segundo para la onda.
fn spawn_level_meter(st: Arc<AppState>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let level = match st.dictation.lock().recording.as_ref() {
                Some(r) => r.mic.level(),
                None => break,
            };
            st.emit("mic-level", level);
        }
    });
}
