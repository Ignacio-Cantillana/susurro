//! Fusión de las dos pistas de una reunión (micrófono = «Yo», sistema = «Otros»)
//! y exportación a Markdown.

use crate::db::{Meeting, Segment};

/// Pausa máxima entre dos segmentos del mismo hablante para unirlos en un turno.
const MERGE_GAP_MS: i64 = 1500;

/// Ordena los segmentos de ambas pistas por tiempo y une los turnos consecutivos
/// del mismo hablante que están separados por menos de `MERGE_GAP_MS`.
pub fn merge_turns(mut segments: Vec<Segment>) -> Vec<Segment> {
    segments.retain(|s| !s.text.trim().is_empty());
    segments.sort_by_key(|s| (s.start_ms, s.end_ms));
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.speaker == seg.speaker && seg.start_ms - last.end_ms <= MERGE_GAP_MS => {
                last.text = format!("{} {}", last.text.trim_end(), seg.text.trim());
                last.end_ms = last.end_ms.max(seg.end_ms);
            }
            _ => out.push(Segment { text: seg.text.trim().to_string(), ..seg }),
        }
    }
    out
}

/// `mm:ss` (o `h:mm:ss` a partir de una hora).
pub fn format_offset(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

/// Transcripción en Markdown, lista para pegar a Claude o guardar.
pub fn to_markdown(meeting: &Meeting, segments: &[Segment]) -> String {
    let mut md = format!("# {}\n\n", meeting.title);
    if let Some(end) = meeting.ended_at {
        md.push_str(&format!("Duración: {}\n\n", format_offset(end - meeting.started_at)));
    }
    for s in merge_turns(segments.to_vec()) {
        md.push_str(&format!("**{}** [{}]: {}\n\n", s.speaker.label(), format_offset(s.start_ms), s.text));
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Speaker::{Me, Others};

    fn seg(speaker: crate::db::Speaker, start: i64, end: i64, text: &str) -> Segment {
        Segment { speaker, start_ms: start, end_ms: end, text: text.into() }
    }

    #[test]
    fn interleaves_tracks_and_merges_close_turns() {
        let me = vec![seg(Me, 0, 2000, "Hola,"), seg(Me, 2500, 4000, "¿qué tal?"), seg(Me, 9000, 10000, "Perfecto.")];
        let others = vec![seg(Others, 4500, 8000, "Muy bien."), seg(Others, 8200, 8800, " ")];
        let merged = merge_turns([me, others].concat());
        assert_eq!(
            merged,
            vec![
                seg(Me, 0, 4000, "Hola, ¿qué tal?"),
                seg(Others, 4500, 8000, "Muy bien."),
                seg(Me, 9000, 10000, "Perfecto."),
            ]
        );
    }

    #[test]
    fn far_apart_same_speaker_stays_split() {
        let merged = merge_turns(vec![seg(Me, 0, 1000, "a"), seg(Me, 5000, 6000, "b")]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn offsets() {
        assert_eq!(format_offset(65_000), "01:05");
        assert_eq!(format_offset(3_725_000), "1:02:05");
    }

    #[test]
    fn markdown_export() {
        let m = Meeting { id: 1, title: "Daily".into(), started_at: 0, ended_at: Some(90_000), audio_path: None, segment_count: 2 };
        let md = to_markdown(&m, &[seg(Others, 3000, 4000, "Vale."), seg(Me, 0, 2000, "Empiezo yo.")]);
        assert_eq!(md, "# Daily\n\nDuración: 01:30\n\n**Yo** [00:00]: Empiezo yo.\n\n**Otros** [00:03]: Vale.\n\n");
    }
}
