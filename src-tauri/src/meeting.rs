//! Grabación de reuniones: dos pistas (micrófono = «Yo», sistema = «Otros») que se
//! trocean en bloques de ~30 s, cortando en silencios, y se transcriben mientras dura
//! la reunión. Al parar solo queda por procesar el último bloque.

use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;
use susurro_core::audio::{WHISPER_RATE, has_speech, quiet_cut_point};
use susurro_core::db::{Segment, Speaker};
use susurro_core::textproc;

use crate::audio::mic::MicCapture;
use crate::audio::system::SystemCapture;
use crate::state::AppState;

const CHUNK_SECS: usize = 30;
/// En los últimos segundos del bloque se busca el silencio donde cortar.
const CUT_SEARCH_MS: u32 = 5_000;

pub struct MeetingSession {
    pub id: i64,
    pub started: Instant,
    pub system_audio: bool,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingStatus {
    /// `idle`, `recording` o `finishing`.
    pub phase: &'static str,
    pub id: Option<i64>,
    pub elapsed_ms: i64,
    pub system_audio: bool,
}

/// Una pista con lo pendiente de transcribir y el instante (ms desde el inicio) en que empieza.
struct Track {
    speaker: Speaker,
    pending: Vec<f32>,
    offset_ms: i64,
    wav: Option<WavFile>,
}

impl Track {
    fn new(speaker: Speaker, wav: Option<WavFile>) -> Self {
        Self { speaker, pending: Vec::new(), offset_ms: 0, wav }
    }

    fn push(&mut self, samples: Vec<f32>) {
        if let Some(w) = self.wav.as_mut() {
            w.append(&samples);
        }
        self.pending.extend(samples);
    }

    /// Saca un bloque listo para transcribir (o todo lo que quede si `flush`).
    fn take_chunk(&mut self, flush: bool) -> Option<(i64, Vec<f32>)> {
        let full = CHUNK_SECS * WHISPER_RATE as usize;
        if self.pending.is_empty() || (!flush && self.pending.len() < full) {
            return None;
        }
        let cut = if flush { self.pending.len() } else { quiet_cut_point(&self.pending, WHISPER_RATE, CUT_SEARCH_MS) };
        let rest = self.pending.split_off(cut);
        let chunk = std::mem::replace(&mut self.pending, rest);
        let offset = self.offset_ms;
        self.offset_ms += (chunk.len() as i64 * 1000) / WHISPER_RATE as i64;
        Some((offset, chunk))
    }
}

pub fn status(st: &AppState) -> MeetingStatus {
    match st.meeting.lock().as_ref() {
        Some(m) if m.stop.load(Ordering::Relaxed) => MeetingStatus {
            phase: "finishing",
            id: Some(m.id),
            elapsed_ms: m.started.elapsed().as_millis() as i64,
            system_audio: m.system_audio,
        },
        Some(m) => MeetingStatus {
            phase: "recording",
            id: Some(m.id),
            elapsed_ms: m.started.elapsed().as_millis() as i64,
            system_audio: m.system_audio,
        },
        None => MeetingStatus { phase: "idle", id: None, elapsed_ms: 0, system_audio: false },
    }
}

fn publish(st: &AppState) {
    st.emit("meeting-status", status(st));
}

pub fn start(st: &Arc<AppState>, title: Option<String>) -> Result<i64> {
    if st.meeting.lock().is_some() {
        anyhow::bail!("ya hay una reunión grabándose");
    }
    let settings = st.settings.read().clone();
    let mic = MicCapture::start(settings.input_device.as_deref())?;
    let system = match SystemCapture::start() {
        Ok(s) => Some(s),
        Err(e) => {
            st.emit("warning", format!("Solo se grabará tu micrófono: {e:#}"));
            None
        }
    };
    let title = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(default_title);
    let id = st.db.lock().create_meeting(&title)?;

    let audio_dir = settings.keep_meeting_audio.then(|| meetings_dir().join(id.to_string()));
    let wav = |name: &str| audio_dir.as_ref().and_then(|d| WavFile::create(d.join(name)).ok());
    let tracks = vec![Track::new(Speaker::Me, wav("yo.wav")), Track::new(Speaker::Others, wav("otros.wav"))];

    let stop = Arc::new(AtomicBool::new(false));
    let system_audio = system.is_some();
    let worker = {
        let (st, stop) = (st.clone(), stop.clone());
        std::thread::Builder::new()
            .name("meeting".into())
            .spawn(move || run(st, id, mic, system, tracks, stop, audio_dir))?
    };
    *st.meeting.lock() = Some(MeetingSession { id, started: Instant::now(), system_audio, stop, worker: Some(worker) });
    st.sound("Morse");
    publish(st);
    spawn_ticker(st.clone());
    Ok(id)
}

/// Pide parar: el hilo de la reunión transcribe lo que falta y la cierra.
pub fn stop(st: &Arc<AppState>) {
    let worker = {
        let mut guard = st.meeting.lock();
        let Some(m) = guard.as_mut() else { return };
        m.stop.store(true, Ordering::Relaxed);
        m.worker.take()
    };
    st.sound("Pop");
    publish(st);
    let st = st.clone();
    std::thread::spawn(move || {
        if let Some(w) = worker {
            let _ = w.join();
        }
        *st.meeting.lock() = None;
        publish(&st);
    });
}

fn run(
    st: Arc<AppState>,
    id: i64,
    mic: MicCapture,
    system: Option<SystemCapture>,
    mut tracks: Vec<Track>,
    stop: Arc<AtomicBool>,
    audio_dir: Option<PathBuf>,
) {
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(500));
        tracks[0].push(mic.drain());
        if let Some(s) = &system {
            tracks[1].push(s.drain());
        }
        for t in tracks.iter_mut() {
            if let Some((offset, chunk)) = t.take_chunk(false) {
                transcribe_chunk(&st, id, t.speaker, offset, &chunk);
            }
        }
    }
    tracks[0].push(mic.stop());
    if let Some(s) = system {
        tracks[1].push(s.stop());
    }
    for t in tracks.iter_mut() {
        while let Some((offset, chunk)) = t.take_chunk(true) {
            transcribe_chunk(&st, id, t.speaker, offset, &chunk);
        }
        if let Some(w) = t.wav.take() {
            w.finish();
        }
    }
    let audio = audio_dir.map(|d| d.to_string_lossy().into_owned());
    if let Err(e) = st.db.lock().finish_meeting(id, audio.as_deref()) {
        st.error(format!("no se pudo cerrar la reunión: {e:#}"));
    }
    st.emit("meeting-updated", id);
}

fn transcribe_chunk(st: &AppState, meeting_id: i64, speaker: Speaker, offset_ms: i64, audio: &[f32]) {
    if !has_speech(audio, WHISPER_RATE) {
        return;
    }
    let result = (|| -> Result<Vec<Segment>> {
        let job = st.job()?;
        let pieces = job.engine.transcribe(audio, &job.request())?;
        Ok(pieces
            .into_iter()
            .filter(|p| !textproc::is_hallucination(&p.text))
            .map(|p| {
                let mut text = textproc::apply_dictionary(&p.text, &job.dict);
                if job.opts.remove_fillers {
                    text = textproc::remove_fillers(&text);
                }
                Segment { speaker, start_ms: offset_ms + p.start_ms, end_ms: offset_ms + p.end_ms, text }
            })
            .filter(|s| !s.text.trim().is_empty())
            .collect())
    })();
    match result.and_then(|segs| st.db.lock().add_segments(meeting_id, &segs)) {
        Ok(()) => st.emit("meeting-updated", meeting_id),
        Err(e) => st.error(format!("error transcribiendo la reunión: {e:#}")),
    }
}

/// Manda el estado cada segundo para el contador del widget.
fn spawn_ticker(st: Arc<AppState>) {
    std::thread::spawn(move || {
        while st.meeting.lock().is_some() {
            publish(&st);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn default_title() -> String {
    let out = std::process::Command::new("date").arg("+Reunión del %d/%m/%Y, %H:%M").output();
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Reunión".into())
}

pub fn meetings_dir() -> PathBuf {
    susurro_core::default_db_path().with_file_name("meetings")
}

/// WAV de 16 bits que se va escribiendo a disco; la cabecera se completa al cerrar.
struct WavFile {
    file: std::io::BufWriter<std::fs::File>,
    samples: u32,
}

impl WavFile {
    fn create(path: PathBuf) -> Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        file.write_all(&susurro_core::audio::encode_wav(&[], WHISPER_RATE))?;
        Ok(Self { file, samples: 0 })
    }

    fn append(&mut self, samples: &[f32]) {
        for s in samples {
            let _ = self.file.write_all(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
        }
        self.samples += samples.len() as u32;
    }

    fn finish(mut self) {
        let data = self.samples * 2;
        let _ = (|| -> std::io::Result<()> {
            self.file.flush()?;
            let f = self.file.get_mut();
            f.seek(SeekFrom::Start(4))?;
            f.write_all(&(36 + data).to_le_bytes())?;
            f.seek(SeekFrom::Start(40))?;
            f.write_all(&data.to_le_bytes())?;
            Ok(())
        })();
    }
}
