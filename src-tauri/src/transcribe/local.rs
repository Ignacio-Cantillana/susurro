use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use parking_lot::Mutex;
use susurro_core::audio::WHISPER_RATE;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{Piece, Request, Transcriber};

/// whisper.cpp con Metal. El modelo se carga una vez y se queda en memoria.
pub struct LocalTranscriber {
    loaded: Mutex<Option<(PathBuf, WhisperContext)>>,
    model_path: Mutex<PathBuf>,
}

impl LocalTranscriber {
    pub fn new(model_path: PathBuf) -> Self {
        whisper_rs::install_logging_hooks();
        Self { loaded: Mutex::new(None), model_path: Mutex::new(model_path) }
    }

    pub fn set_model(&self, path: PathBuf) {
        *self.model_path.lock() = path;
    }

    /// Carga el modelo si hace falta (tarda unos segundos la primera vez).
    pub fn warm_up(&self) -> Result<()> {
        let path = self.model_path.lock().clone();
        let mut loaded = self.loaded.lock();
        if loaded.as_ref().is_some_and(|(p, _)| *p == path) {
            return Ok(());
        }
        *loaded = None; // libera el anterior antes de cargar otro
        *loaded = Some((path.clone(), load(&path)?));
        Ok(())
    }

    pub fn unload(&self) {
        *self.loaded.lock() = None;
    }
}

fn load(path: &Path) -> Result<WhisperContext> {
    if !path.is_file() {
        return Err(anyhow!("el modelo no está descargado: {}", path.display()));
    }
    let mut params = WhisperContextParameters::default();
    params.flash_attn(true); // ~35 % más rápido en Apple Silicon (medido)
    WhisperContext::new_with_params(path.to_string_lossy().as_ref(), params)
        .with_context(|| format!("no se pudo cargar {}", path.display()))
}

impl Transcriber for LocalTranscriber {
    fn transcribe(&self, audio: &[f32], req: &Request) -> Result<Vec<Piece>> {
        self.warm_up()?;
        let loaded = self.loaded.lock();
        let (_, ctx) = loaded.as_ref().expect("cargado en warm_up");
        let mut state = ctx.create_state()?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(req.language.or(Some("auto")));
        if let Some(prompt) = req.prompt.filter(|p| !p.contains('\0')) {
            params.set_initial_prompt(prompt);
        }
        params.set_n_threads(std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4));
        // No usamos `audio_ctx` reducido: es más rápido pero repite frases (medido).
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // whisper.cpp descarta audios de menos de 1 s: se rellena con silencio.
        let min = WHISPER_RATE as usize + WHISPER_RATE as usize / 4;
        let padded;
        let audio = if audio.len() < min {
            padded = [audio, &vec![0.0; min - audio.len()]].concat();
            &padded[..]
        } else {
            audio
        };

        state.full(params, audio)?;
        let mut pieces = Vec::new();
        for seg in state.as_iter() {
            if seg.no_speech_probability() > 0.8 {
                continue;
            }
            pieces.push(Piece {
                // whisper.cpp devuelve centésimas de segundo.
                start_ms: seg.start_timestamp() * 10,
                end_ms: seg.end_timestamp() * 10,
                text: seg.to_str_lossy()?.trim().to_string(),
            });
        }
        Ok(pieces)
    }
}
