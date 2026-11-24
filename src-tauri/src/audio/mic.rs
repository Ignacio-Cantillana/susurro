//! Captura del micrófono con cpal. El stream vive en su propio hilo (en macOS no es
//! `Send`) y deja las muestras mono, a la frecuencia nativa, en un búfer compartido.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};
use parking_lot::Mutex;
use susurro_core::audio::{WHISPER_RATE, resample, rms, to_mono};

pub struct MicCapture {
    buf: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    rate: u32,
    stop_tx: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.description().ok().map(|desc| desc.name().to_string())).collect())
        .unwrap_or_default()
}

fn pick_device(name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name {
        if let Ok(mut devs) = host.input_devices() {
            if let Some(d) = devs.find(|d| d.description().is_ok_and(|desc| desc.name() == name)) {
                return Ok(d);
            }
        }
        log::warn!("micrófono «{name}» no encontrado, uso el predeterminado");
    }
    host.default_input_device().ok_or_else(|| anyhow!("no hay ningún micrófono disponible"))
}

impl MicCapture {
    pub fn start(device_name: Option<&str>) -> Result<Self> {
        let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(WHISPER_RATE as usize * 60 * 3)));
        let level = Arc::new(AtomicU32::new(0));
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32>>();
        let device_name = device_name.map(str::to_owned);
        let (buf2, level2) = (buf.clone(), level.clone());

        let thread = std::thread::Builder::new().name("mic".into()).spawn(move || {
            let stream = match build_stream(device_name.as_deref(), buf2, level2) {
                Ok((stream, rate)) => {
                    let _ = ready_tx.send(Ok(rate));
                    stream
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = stop_rx.recv(); // espera a stop() o a que se suelte el Sender
            drop(stream);
        })?;

        let rate = ready_rx.recv().context("el hilo del micrófono murió")??;
        Ok(Self { buf, level, rate, stop_tx, thread: Some(thread) })
    }

    /// Nivel de la señal (RMS) de los últimos milisegundos.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Saca todo lo grabado hasta ahora, ya a 16 kHz.
    pub fn drain(&self) -> Vec<f32> {
        let raw = std::mem::take(&mut *self.buf.lock());
        resample(&raw, self.rate, WHISPER_RATE)
    }

    /// Para la captura y devuelve lo que quedaba en el búfer, a 16 kHz.
    pub fn stop(mut self) -> Vec<f32> {
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.drain()
    }
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
    }
}

fn build_stream(
    device_name: Option<&str>,
    buf: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<(cpal::Stream, u32)> {
    let device = pick_device(device_name)?;
    let config = device.default_input_config().context("el micrófono no da una configuración válida")?;
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let err_fn = |e: cpal::Error| log::error!("error del micrófono: {e}");

    fn push<T: Sample>(data: &[T], channels: usize, buf: &Mutex<Vec<f32>>, level: &AtomicU32)
    where
        f32: FromSample<T>,
    {
        let floats: Vec<f32> = data.iter().map(|s| f32::from_sample(*s)).collect();
        let mono = to_mono(&floats, channels);
        level.store(rms(&mono).to_bits(), Ordering::Relaxed);
        buf.lock().extend_from_slice(&mono);
    }

    let cfg: cpal::StreamConfig = config.into();
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            cfg,
            move |d: &[f32], _: &_| push(d, channels, &buf, &level),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            cfg,
            move |d: &[i16], _: &_| push(d, channels, &buf, &level),
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            cfg,
            move |d: &[i32], _: &_| push(d, channels, &buf, &level),
            err_fn,
            None,
        ),
        other => return Err(anyhow!("formato de micrófono no soportado: {other}")),
    }
    .context("no se pudo abrir el micrófono (¿has dado permiso?)")?;
    stream.play()?;
    Ok((stream, rate))
}
