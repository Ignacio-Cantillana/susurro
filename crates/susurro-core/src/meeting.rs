//! Fusión de las dos pistas de una reunión (micrófono = «Yo», sistema = «Otros»)
//! y exportación a Markdown.

use crate::db::{DictTerm, Meeting, Segment};

/// Pausa máxima entre dos segmentos del mismo hablante para unirlos en un turno.
const MERGE_GAP_MS: i64 = 1500;

/// Limpia y ordena los segmentos de ambas pistas: quita de «Yo» el eco de «Otros»
/// (ver [`crate::echo`]), descarta los sueltos sin contenido («Y», «Eh») y une los
/// turnos consecutivos del mismo hablante separados por menos de `MERGE_GAP_MS`.
pub fn merge_turns(segments: Vec<Segment>) -> Vec<Segment> {
    let mut segments = crate::echo::remove_echo(segments);
    segments.retain(|s| !s.text.trim().is_empty() && !is_filler_only(&s.text));
    segments.sort_by_key(|s| (s.start_ms, s.end_ms));
    let lower = lowercase_words(&segments);
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.speaker == seg.speaker && seg.start_ms - last.end_ms <= MERGE_GAP_MS => {
                let next = continue_sentence(&last.text, seg.text.trim(), &lower);
                last.text = format!("{} {}", last.text.trim_end(), next);
                last.end_ms = last.end_ms.max(seg.end_ms);
            }
            _ => out.push(Segment { text: capitalize(seg.text.trim()), ..seg }),
        }
    }
    out
}

/// Turnos listos para leer: [`merge_turns`] y luego el diccionario propio, para que
/// un término añadido hoy corrija también las reuniones ya grabadas.
pub fn transcript(segments: Vec<Segment>, dict: &[DictTerm]) -> Vec<Segment> {
    let mut turns = merge_turns(segments);
    if !dict.is_empty() {
        for t in &mut turns {
            t.text = crate::textproc::apply_dictionary(&t.text, dict);
        }
    }
    turns
}

/// Un turno empieza en mayúscula aunque Whisper lo cortara a media frase.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => c.to_uppercase().chain(chars).collect(),
        _ => text.to_string(),
    }
}

/// Palabras sueltas que Whisper deja al cortar un bloque y no dicen nada.
fn is_filler_only(text: &str) -> bool {
    const FILLERS: &[&str] = &["y", "e", "o", "u", "que", "pues", "eh", "em", "ehm", "mm", "mmm", "ah", "and", "the", "um", "uh"];
    let words: Vec<String> =
        text.split_whitespace().map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()).collect();
    words.len() <= 2 && words.iter().all(|w| w.is_empty() || FILLERS.contains(&w.as_str()))
}

/// Palabras que aparecen en minúscula en algún sitio de la reunión: si una de ellas
/// empieza un segmento con mayúscula, es por el corte de Whisper, no un nombre propio.
fn lowercase_words(segments: &[Segment]) -> std::collections::HashSet<String> {
    segments
        .iter()
        .flat_map(|s| s.text.split_whitespace())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().next().is_some_and(char::is_lowercase))
        .map(str::to_string)
        .collect()
}

/// «Tendría que continuar» + «Con esto» → «con esto»: si la frase anterior no
/// terminó, la siguiente no empieza en mayúscula (salvo nombres propios).
fn continue_sentence(prev: &str, next: &str, lower: &std::collections::HashSet<String>) -> String {
    let ended = prev.trim_end().ends_with(['.', '?', '!', '…', ':', '"', '»']);
    let first = next.split_whitespace().next().unwrap_or("");
    let bare = first.trim_matches(|c: char| !c.is_alphanumeric());
    let Some(c) = bare.chars().next() else { return next.to_string() };
    if ended || !c.is_uppercase() || bare.chars().skip(1).any(char::is_uppercase) {
        return next.to_string();
    }
    let candidate: String = c.to_lowercase().chain(bare.chars().skip(1)).collect();
    if !lower.contains(&candidate) {
        return next.to_string();
    }
    next.replacen(bare, &candidate, 1)
}

/// `mm:ss` (o `h:mm:ss` a partir de una hora).
pub fn format_offset(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

/// Transcripción en Markdown, lista para pegar a Claude o guardar.
pub fn to_markdown(meeting: &Meeting, segments: &[Segment], dict: &[DictTerm]) -> String {
    let mut md = format!("# {}\n\n", meeting.title);
    if let Some(end) = meeting.ended_at {
        md.push_str(&format!("Duración: {}\n\n", format_offset(end - meeting.started_at)));
    }
    for s in transcript(segments.to_vec(), dict) {
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
    fn joins_cut_sentences_in_lowercase_and_keeps_names() {
        let merged = merge_turns(vec![
            seg(Others, 0, 1000, "Tendría que continuar"),
            seg(Others, 1100, 2000, "Con esto oscurito"),
            seg(Others, 2100, 3000, "y luego"),
            seg(Others, 3100, 4000, "Miguel lo monta"),
            seg(Others, 4100, 5000, "con calma."),
            seg(Others, 5100, 6000, "Con eso vale."),
        ]);
        assert_eq!(merged[0].text, "Tendría que continuar con esto oscurito y luego Miguel lo monta con calma. Con eso vale.");
    }

    #[test]
    fn transcript_applies_dictionary_and_capitalizes_turns() {
        let dict = [DictTerm { id: 1, term: "Claude".into(), sounds_like: "Cloud".into() }];
        let turns = transcript(vec![seg(Me, 0, 1000, "con cloud lo sacas")], &dict);
        assert_eq!(turns[0].text, "Con Claude lo sacas");
    }

    #[test]
    fn drops_loose_fillers() {
        let merged = merge_turns(vec![seg(Me, 0, 1000, "Perfecto."), seg(Me, 9000, 9500, "Y")]);
        assert_eq!(merged, vec![seg(Me, 0, 1000, "Perfecto.")]);
    }

    #[test]
    fn offsets() {
        assert_eq!(format_offset(65_000), "01:05");
        assert_eq!(format_offset(3_725_000), "1:02:05");
    }

    #[test]
    fn markdown_export() {
        let m = Meeting { id: 1, title: "Daily".into(), started_at: 0, ended_at: Some(90_000), audio_path: None, segment_count: 2 };
        let md = to_markdown(&m, &[seg(Others, 3000, 4000, "Vale."), seg(Me, 0, 2000, "Empiezo yo.")], &[]);
        assert_eq!(md, "# Daily\n\nDuración: 01:30\n\n**Yo** [00:00]: Empiezo yo.\n\n**Otros** [00:03]: Vale.\n\n");
    }
}
