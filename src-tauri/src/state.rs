use std::sync::Arc;

use anyhow::{Result, anyhow};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use susurro_core::{Db, textproc};
use tauri::{AppHandle, Emitter};

use crate::dictation::Dictation;
use crate::meeting::MeetingSession;
use crate::settings::{Provider, Settings};
use crate::transcribe::{ApiTranscriber, LocalTranscriber, Request, Transcriber};
use crate::{models, secrets};

pub struct AppState {
    pub app: AppHandle,
    pub db: Mutex<Db>,
    pub settings: RwLock<Settings>,
    pub local: Arc<LocalTranscriber>,
    pub dictation: Mutex<Dictation>,
    pub meeting: Mutex<Option<MeetingSession>>,
}

/// Lo que necesita un hilo de trabajo para transcribir y postprocesar,
/// capturado una vez para no tener los locks abiertos mientras corre Whisper.
pub struct TranscribeJob {
    pub engine: Arc<dyn Transcriber>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub dict: Vec<susurro_core::db::DictTerm>,
    pub snippets: Vec<susurro_core::db::Snippet>,
    pub opts: textproc::Options,
}

impl TranscribeJob {
    pub fn request(&self) -> Request<'_> {
        Request { language: self.language.as_deref(), prompt: self.prompt.as_deref() }
    }
}

impl AppState {
    pub fn new(app: AppHandle, db: Db) -> Self {
        let settings = Settings::load(&db);
        crate::hotkey::set_hotkey(settings.hotkey);
        let model_path = models::find(&settings.local_model)
            .map(models::path_of)
            .unwrap_or_else(|| models::path_of(&models::CATALOG[0]));
        Self {
            app,
            db: Mutex::new(db),
            settings: RwLock::new(settings),
            local: Arc::new(LocalTranscriber::new(model_path)),
            dictation: Mutex::new(Dictation::default()),
            meeting: Mutex::new(None),
        }
    }

    pub fn engine(&self) -> Result<Arc<dyn Transcriber>> {
        let provider = self.settings.read().provider;
        Ok(match provider {
            Provider::Local => self.local.clone(),
            Provider::Groq => Arc::new(ApiTranscriber::groq(
                secrets::get("groq").ok_or_else(|| anyhow!("falta la clave de Groq en Ajustes"))?,
            )),
            Provider::Openai => Arc::new(ApiTranscriber::openai(
                secrets::get("openai").ok_or_else(|| anyhow!("falta la clave de OpenAI en Ajustes"))?,
            )),
        })
    }

    pub fn job(&self) -> Result<TranscribeJob> {
        let engine = self.engine()?;
        let settings = self.settings.read().clone();
        let db = self.db.lock();
        let dict = db.list_dictionary()?;
        let snippets = db.list_snippets()?;
        Ok(TranscribeJob {
            engine,
            language: settings.whisper_language().map(str::to_owned),
            prompt: textproc::build_prompt(&dict),
            dict,
            snippets,
            opts: textproc::Options { remove_fillers: settings.remove_fillers },
        })
    }

    /// Borra los dictados más antiguos que lo que diga Ajustes (24 h por defecto).
    /// Las notas de voz no se tocan.
    pub fn purge_old_dictations(&self) {
        let hours = self.settings.read().dictation_retention_hours;
        if hours == 0 {
            return;
        }
        let before = susurro_core::now_ms() - i64::from(hours) * 3_600_000;
        match self.db.lock().purge_entries(susurro_core::db::EntryKind::Dictation, before) {
            Ok(0) => {}
            Ok(n) => {
                log::info!("borrados {n} dictados de más de {hours} h");
                self.emit("entries-changed", serde_json::json!({ "kind": "dictation", "purged": n }));
            }
            Err(e) => log::warn!("no se pudieron borrar los dictados viejos: {e:#}"),
        }
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        let _ = self.app.emit(event, payload);
    }

    pub fn error(&self, msg: impl std::fmt::Display) {
        log::error!("{msg}");
        self.emit("error", msg.to_string());
        self.sound("Basso");
    }

    pub fn sound(&self, name: &str) {
        if self.settings.read().sounds {
            let _ = std::process::Command::new("/usr/bin/afplay")
                .arg(format!("/System/Library/Sounds/{name}.aiff"))
                .spawn();
        }
    }
}
