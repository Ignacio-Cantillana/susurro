//! Motores de transcripción intercambiables: whisper.cpp en local o una API
//! compatible con OpenAI (Groq, OpenAI).

mod api;
mod local;

pub use api::ApiTranscriber;
pub use local::LocalTranscriber;

/// Un tramo de texto con tiempos relativos al inicio del audio.
#[derive(Debug, Clone)]
pub struct Piece {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub language: Option<&'a str>,
    /// Contexto para Whisper: términos del diccionario.
    pub prompt: Option<&'a str>,
}

pub trait Transcriber: Send + Sync {
    /// `audio`: f32 mono a 16 kHz.
    fn transcribe(&self, audio: &[f32], req: &Request) -> anyhow::Result<Vec<Piece>>;
}

pub fn join(pieces: &[Piece]) -> String {
    pieces.iter().map(|p| p.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
}
