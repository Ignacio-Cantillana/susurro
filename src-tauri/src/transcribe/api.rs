use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, multipart};
use serde::Deserialize;
use susurro_core::audio::{WHISPER_RATE, encode_wav};

use super::{Piece, Request, Transcriber};

/// Cualquier endpoint `/audio/transcriptions` compatible con OpenAI.
pub struct ApiTranscriber {
    client: Client,
    base_url: &'static str,
    model: &'static str,
    api_key: String,
    /// Si el modelo admite `verbose_json` (devuelve segmentos con tiempos).
    verbose: bool,
}

impl ApiTranscriber {
    pub fn groq(api_key: String) -> Self {
        Self::new("https://api.groq.com/openai/v1", "whisper-large-v3-turbo", api_key, true)
    }

    pub fn openai(api_key: String) -> Self {
        Self::new("https://api.openai.com/v1", "gpt-4o-mini-transcribe", api_key, false)
    }

    fn new(base_url: &'static str, model: &'static str, api_key: String, verbose: bool) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("cliente HTTP");
        Self { client, base_url, model, api_key, verbose }
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    text: String,
    #[serde(default)]
    segments: Vec<ApiSegment>,
}

#[derive(Deserialize)]
struct ApiSegment {
    start: f64,
    end: f64,
    text: String,
}

impl Transcriber for ApiTranscriber {
    fn transcribe(&self, audio: &[f32], req: &Request) -> Result<Vec<Piece>> {
        let wav = encode_wav(audio, WHISPER_RATE);
        let mut form = multipart::Form::new()
            .text("model", self.model)
            .text("response_format", if self.verbose { "verbose_json" } else { "json" })
            .part("file", multipart::Part::bytes(wav).file_name("audio.wav").mime_str("audio/wav")?);
        if let Some(lang) = req.language {
            form = form.text("language", lang.to_string());
        }
        if let Some(prompt) = req.prompt {
            form = form.text("prompt", prompt.to_string());
        }
        let resp = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .context("no se pudo conectar con la API de transcripción")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("la API respondió {status}: {}", body.chars().take(300).collect::<String>());
        }
        let parsed: ApiResponse = resp.json()?;
        let duration_ms = (audio.len() as i64 * 1000) / WHISPER_RATE as i64;
        if parsed.segments.is_empty() {
            return Ok(vec![Piece { start_ms: 0, end_ms: duration_ms, text: parsed.text.trim().to_string() }]);
        }
        Ok(parsed
            .segments
            .into_iter()
            .map(|s| Piece {
                start_ms: (s.start * 1000.0) as i64,
                end_ms: (s.end * 1000.0) as i64,
                text: s.text.trim().to_string(),
            })
            .collect())
    }
}
