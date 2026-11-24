//! Utilidades de audio sin dependencias: Whisper quiere f32 mono a 16 kHz.

pub const WHISPER_RATE: u32 = 16_000;

/// Mezcla muestras intercaladas de `channels` canales a mono.
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Remuestreo con filtro paso bajo (media móvil del tamaño del factor de diezmado)
/// e interpolación lineal. Para voz hacia 16 kHz es suficiente y no añade dependencias.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let filtered: Vec<f32> = if ratio > 1.0 {
        let win = ratio.round().max(1.0) as usize;
        let mut out = Vec::with_capacity(input.len());
        let mut acc = 0.0f32;
        for i in 0..input.len() {
            acc += input[i];
            if i >= win {
                acc -= input[i - win];
            }
            out.push(acc / (i + 1).min(win) as f32);
        }
        out
    } else {
        input.to_vec()
    };
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = filtered[idx.min(filtered.len() - 1)];
            let b = filtered[(idx + 1).min(filtered.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// Nivel RMS (0..1 aprox.) para pintar la onda y detectar voz.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// ¿Hay voz en este audio? Umbral de energía por ventanas de 30 ms: basta con que
/// un ~5 % de las ventanas lo supere. Evita mandar silencio a Whisper (que alucina).
pub fn has_speech(samples: &[f32], rate: u32) -> bool {
    let win = (rate as usize * 30 / 1000).max(1);
    let windows: Vec<f32> = samples.chunks(win).map(rms).collect();
    if windows.is_empty() {
        return false;
    }
    let loud = windows.iter().filter(|&&r| r > 0.01).count();
    loud * 20 >= windows.len() && loud >= 3
}

/// Busca el mejor punto de corte (la ventana más silenciosa) en el último tramo de
/// `search_ms` del audio, para no cortar palabras al trocear una reunión.
pub fn quiet_cut_point(samples: &[f32], rate: u32, search_ms: u32) -> usize {
    let win = (rate as usize * 50 / 1000).max(1);
    let search = (rate as usize * search_ms as usize / 1000).min(samples.len());
    let start = samples.len() - search;
    let mut best = (f32::MAX, samples.len());
    let mut i = start;
    while i + win <= samples.len() {
        let r = rms(&samples[i..i + win]);
        if r < best.0 {
            best = (r, i + win / 2);
        }
        i += win;
    }
    best.1
}

/// WAV PCM 16 bits mono, para mandar a las APIs de transcripción.
pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // tamaño del bloque fmt
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // bytes por segundo
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits por muestra
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// Lee un WAV PCM de 16 bits (cualquier número de canales y frecuencia) y lo
/// devuelve como f32 mono a 16 kHz. Recorre los chunks hasta encontrar `fmt ` y `data`.
pub fn decode_wav(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut channels, mut rate, mut bits) = (0usize, 0u32, 0u16);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16_at(body + 2) as usize;
            rate = u32_at(body + 4);
            bits = u16_at(body + 14);
        } else if id == b"data" {
            if bits != 16 || channels == 0 {
                return None;
            }
            let end = (body + size).min(bytes.len());
            let samples: Vec<f32> = bytes[body..end]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / i16::MAX as f32)
                .collect();
            return Some(resample(&to_mono(&samples, channels), rate, WHISPER_RATE));
        }
        pos = body + size + (size & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn mono_mix() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(to_mono(&[0.3], 1), vec![0.3]);
    }

    #[test]
    fn resample_keeps_duration_and_tone() {
        let input = sine(440.0, 48_000, 1.0, 0.5);
        let out = resample(&input, 48_000, WHISPER_RATE);
        assert_eq!(out.len(), 16_000);
        // Un tono de 440 Hz sobrevive al filtro con casi la misma energía.
        assert!((rms(&out) - rms(&input)).abs() < 0.05, "{} vs {}", rms(&out), rms(&input));
        assert_eq!(resample(&input, 48_000, 48_000).len(), input.len());
        assert_eq!(resample(&sine(200.0, 44_100, 0.5, 0.5), 44_100, WHISPER_RATE).len(), 8_000);
    }

    #[test]
    fn speech_detection() {
        assert!(!has_speech(&vec![0.0; 16_000], WHISPER_RATE));
        assert!(!has_speech(&vec![0.002; 16_000], WHISPER_RATE), "ruido de fondo no es voz");
        assert!(has_speech(&sine(220.0, WHISPER_RATE, 1.0, 0.2), WHISPER_RATE));
    }

    #[test]
    fn cut_point_lands_in_silence() {
        let mut audio = sine(220.0, WHISPER_RATE, 2.0, 0.3);
        let gap_start = audio.len();
        audio.extend(vec![0.0; 8_000]); // medio segundo de silencio
        audio.extend(sine(220.0, WHISPER_RATE, 1.0, 0.3));
        let cut = quiet_cut_point(&audio, WHISPER_RATE, 3_000);
        assert!(cut > gap_start && cut < gap_start + 8_000, "corte en {cut}");
    }

    #[test]
    fn wav_roundtrip() {
        let input = sine(220.0, WHISPER_RATE, 0.5, 0.5);
        let decoded = decode_wav(&encode_wav(&input, WHISPER_RATE)).unwrap();
        assert_eq!(decoded.len(), input.len());
        assert!((decoded[100] - input[100]).abs() < 0.001);
        assert!(decode_wav(b"not a wav").is_none());
    }

    #[test]
    fn wav_header() {
        let wav = encode_wav(&[0.0, 1.0, -1.0], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), i16::MAX);
    }
}
