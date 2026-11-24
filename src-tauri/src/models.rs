//! Catálogo de modelos de whisper.cpp y su descarga desde Hugging Face.

use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub size_mb: u32,
    pub description: &'static str,
}

pub const CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "large-v3-turbo-q5",
        name: "Large v3 Turbo (q5)",
        file: "ggml-large-v3-turbo-q5_0.bin",
        size_mb: 574,
        description: "Recomendado. Casi la calidad del large y muy rápido en Apple Silicon.",
    },
    ModelInfo {
        id: "large-v3-turbo",
        name: "Large v3 Turbo",
        file: "ggml-large-v3-turbo.bin",
        size_mb: 1624,
        description: "Máxima calidad. Necesita más memoria.",
    },
    ModelInfo {
        id: "small-q5",
        name: "Small (q5)",
        file: "ggml-small-q5_1.bin",
        size_mb: 190,
        description: "Ligero, para Macs con poca memoria. Peor con nombres propios.",
    },
    ModelInfo {
        id: "base-q5",
        name: "Base (q5)",
        file: "ggml-base-q5_1.bin",
        size_mb: 60,
        description: "El más rápido, para pruebas.",
    },
];

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

pub fn models_dir() -> PathBuf {
    susurro_core::default_db_path().with_file_name("models")
}

pub fn path_of(model: &ModelInfo) -> PathBuf {
    models_dir().join(model.file)
}

pub fn is_downloaded(model: &ModelInfo) -> bool {
    path_of(model).is_file()
}

/// Descarga el modelo a `<models>/<file>.part` y lo renombra al terminar, para que
/// una descarga cortada nunca se tome por un modelo válido.
pub fn download(model: &ModelInfo, mut on_progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    let dest = path_of(model);
    if dest.is_file() {
        return Ok(dest);
    }
    std::fs::create_dir_all(models_dir())?;
    let tmp = dest.with_extension("part");
    let client = reqwest::blocking::Client::builder().timeout(None).build()?;
    let mut resp = client
        .get(format!("{BASE_URL}/{}", model.file))
        .send()
        .context("no se pudo conectar con Hugging Face")?;
    if !resp.status().is_success() {
        bail!("Hugging Face respondió {}", resp.status());
    }
    let total = resp.content_length().unwrap_or(model.size_mb as u64 * 1_000_000);
    let mut file = std::fs::File::create(&tmp)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    let mut last_report = 0u64;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if done - last_report >= 4 << 20 {
            on_progress(done, total);
            last_report = done;
        }
    }
    file.flush()?;
    if done < total / 2 {
        let _ = std::fs::remove_file(&tmp);
        bail!("descarga incompleta ({done} de {total} bytes)");
    }
    std::fs::rename(&tmp, &dest)?;
    on_progress(total, total);
    Ok(dest)
}

pub fn delete(model: &ModelInfo) -> Result<()> {
    let p = path_of(model);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}
