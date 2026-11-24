//! Ajustes de la app, guardados como JSON en la tabla `settings` de SQLite.

use serde::{Deserialize, Serialize};
use susurro_core::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hotkey {
    Fn,
    RightOption,
    RightCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Local,
    Groq,
    Openai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Código ISO del idioma («es», «en»…) o «auto».
    pub language: String,
    pub hotkey: Hotkey,
    pub provider: Provider,
    /// Id del modelo local (ver `models::CATALOG`).
    pub local_model: String,
    pub remove_fillers: bool,
    pub sounds: bool,
    pub show_widget: bool,
    /// Guardar el audio de las reuniones en disco.
    pub keep_meeting_audio: bool,
    /// Nombre del micrófono; `None` = el predeterminado del sistema.
    pub input_device: Option<String>,
    pub onboarding_done: bool,
    /// Horas que se guardan los dictados en el historial; 0 = siempre.
    /// Las notas de voz no caducan nunca.
    pub dictation_retention_hours: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "es".into(),
            hotkey: Hotkey::Fn,
            provider: Provider::Local,
            local_model: "large-v3-turbo-q5".into(),
            remove_fillers: true,
            sounds: true,
            show_widget: true,
            keep_meeting_audio: false,
            input_device: None,
            onboarding_done: false,
            dictation_retention_hours: 24,
        }
    }
}

const KEY: &str = "app_settings";

impl Settings {
    pub fn load(db: &Db) -> Self {
        db.get_setting(KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, db: &Db) -> anyhow::Result<()> {
        db.set_setting(KEY, &serde_json::to_string(self)?)
    }

    pub fn whisper_language(&self) -> Option<&str> {
        (self.language != "auto").then_some(self.language.as_str())
    }
}
