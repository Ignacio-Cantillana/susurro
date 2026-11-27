//! Limpieza del eco entre pistas de una reunión.
//!
//! Sin cascos, el micrófono capta por los altavoces la voz de los demás: cada frase
//! sale dos veces, una en «Otros» (limpia, del audio del sistema) y otra en «Yo»
//! (con ~0,5-1 s de retraso y peor transcrita). Aquí se buscan, por texto, los tramos
//! de «Yo» que repiten lo que dice «Otros» en esa misma ventana de tiempo y se quitan,
//! quedándonos con la versión del sistema. Lo que «Yo» dice de verdad se conserva,
//! aunque venga pegado a un tramo de eco en el mismo segmento.

use crate::db::{Segment, Speaker};

/// Margen alrededor de un segmento de «Yo» en el que se busca su eco en «Otros».
const WINDOW_MS: i64 = 2_000;
/// Un tramo de eco necesita al menos estas palabras coincidentes seguidas.
const MIN_RUN: usize = 3;
/// Si el eco cubre esta fracción del segmento, se quita entero.
const DROP_COVERAGE: f32 = 0.6;
/// Lo que sobra entre tramos de eco solo se queda si tiene al menos estas palabras.
const MIN_KEEP: usize = 3;

/// Quita de la pista «Yo» lo que es eco de «Otros». Si no hay pista «Otros», no toca nada.
pub fn remove_echo(segments: Vec<Segment>) -> Vec<Segment> {
    let mut others: Vec<&Segment> = segments.iter().filter(|s| s.speaker == Speaker::Others).collect();
    if others.is_empty() {
        return segments;
    }
    others.sort_by_key(|s| s.start_ms);
    let mut out = Vec::with_capacity(segments.len());
    for seg in &segments {
        if seg.speaker != Speaker::Me {
            out.push(seg.clone());
            continue;
        }
        let near: Vec<&Segment> = others
            .iter()
            .copied()
            .filter(|o| o.end_ms >= seg.start_ms - WINDOW_MS && o.start_ms <= seg.end_ms + WINDOW_MS)
            .collect();
        out.extend(strip(seg, &near));
    }
    out
}

/// Fracción de la pista «Yo» que era eco (0..1), por palabras. Sirve para avisar
/// de que la reunión se grabó sin cascos.
pub fn echo_ratio(segments: &[Segment]) -> f32 {
    let words = |segs: &[Segment]| -> usize {
        segs.iter().filter(|s| s.speaker == Speaker::Me).map(|s| s.text.split_whitespace().count()).sum()
    };
    let before = words(segments);
    if before == 0 {
        return 0.0;
    }
    let after = words(&remove_echo(segments.to_vec()));
    1.0 - after as f32 / before as f32
}

fn strip(seg: &Segment, near: &[&Segment]) -> Vec<Segment> {
    let words: Vec<&str> = seg.text.split_whitespace().collect();
    let mine: Vec<String> = words.iter().map(|w| norm(w)).collect();
    let theirs: Vec<String> =
        near.iter().flat_map(|o| o.text.split_whitespace().map(norm).collect::<Vec<_>>()).filter(|w| !w.is_empty()).collect();
    if mine.iter().all(|w| w.is_empty()) || theirs.is_empty() {
        return vec![seg.clone()];
    }

    // Segmentos muy cortos («Sí.», «Vale.»): solo son eco si «Otros» dice exactamente
    // lo mismo justo a la vez; si no, son respuestas de verdad.
    let content: Vec<&String> = mine.iter().filter(|w| !w.is_empty()).collect();
    if content.len() < MIN_RUN {
        let same = near.iter().any(|o| {
            let t: Vec<String> = o.text.split_whitespace().map(norm).filter(|w| !w.is_empty()).collect();
            t.iter().collect::<Vec<_>>() == content && (o.start_ms - seg.start_ms).abs() <= WINDOW_MS
        });
        return if same { vec![] } else { vec![seg.clone()] };
    }

    let echo = echo_mask(&mine, &theirs);
    let echoed = echo.iter().filter(|&&e| e).count();
    if echoed == 0 {
        return vec![seg.clone()];
    }
    if echoed as f32 / words.len() as f32 >= DROP_COVERAGE {
        return vec![];
    }

    // Quedan trozos propios entre el eco: cada uno con su parte proporcional del tiempo.
    let dur = (seg.end_ms - seg.start_ms).max(0);
    let at = |i: usize| seg.start_ms + dur * i as i64 / words.len() as i64;
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if echo[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < words.len() && !echo[i] {
            i += 1;
        }
        if mine[start..i].iter().filter(|w| !w.is_empty()).count() >= MIN_KEEP {
            out.push(Segment { speaker: seg.speaker, start_ms: at(start), end_ms: at(i), text: words[start..i].join(" ") });
        }
    }
    out
}

/// Marca qué palabras de `mine` forman parte de un tramo que también está en `theirs`.
/// Alinea con la subsecuencia común más larga y se queda solo con los tramos de
/// `MIN_RUN` coincidencias o más (se admite un hueco de una palabra dentro del tramo,
/// porque Whisper no transcribe igual las dos versiones: «se va» / «se te va»).
fn echo_mask(mine: &[String], theirs: &[String]) -> Vec<bool> {
    let (n, m) = (mine.len(), theirs.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if !mine[i].is_empty() && mine[i] == theirs[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    // Pares (i, j) emparejados, en orden.
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if !mine[i].is_empty() && mine[i] == theirs[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Agrupa en tramos: dos parejas seguidas van juntas si están cerca en ambas pistas.
    let mut mask = vec![false; n];
    let mut run: Vec<(usize, usize)> = Vec::new();
    let flush = |run: &mut Vec<(usize, usize)>, mask: &mut Vec<bool>| {
        if run.len() >= MIN_RUN {
            let (a, b) = (run[0].0, run[run.len() - 1].0);
            mask[a..=b].iter_mut().for_each(|x| *x = true);
        }
        run.clear();
    };
    for &(i, j) in &pairs {
        if let Some(&(pi, pj)) = run.last() {
            if i - pi > 2 || j - pj > 3 {
                flush(&mut run, &mut mask);
            }
        }
        run.push((i, j));
    }
    flush(&mut run, &mut mask);
    // Las palabras vacías (solo puntuación) pegadas al eco también son eco.
    for k in 0..n {
        if mine[k].is_empty() && ((k > 0 && mask[k - 1]) || (k + 1 < n && mask[k + 1])) {
            mask[k] = true;
        }
    }
    mask
}

/// Minúsculas, sin tildes ni puntuación: «¿Sabes?» → «sabes», «Sí,» → «si».
fn norm(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Speaker::{Me, Others};

    fn seg(speaker: Speaker, start: i64, end: i64, text: &str) -> Segment {
        Segment { speaker, start_ms: start, end_ms: end, text: text.into() }
    }

    #[test]
    fn drops_mic_echo_of_system_audio() {
        let segs = vec![
            seg(Others, 3640, 6180, "Que el fondo sea continuado"),
            seg(Others, 6180, 7900, "Porque si no aquí se te va a dar un corte"),
            seg(Others, 7900, 9020, "De esta imagen con esta."),
            seg(Me, 5500, 9500, "Continuado porque si no aquí se va a dar un corte de esta imagen con esta"),
        ];
        let out = remove_echo(segs);
        assert!(out.iter().all(|s| s.speaker == Others), "{out:?}");
    }

    #[test]
    fn keeps_own_speech_next_to_echo() {
        let segs = vec![
            seg(Others, 0, 4000, "Y ya está, yo creo que lo demás está bien"),
            seg(Me, 500, 9000, "Y ya está yo creo que lo demás está bien. Vale, el diseño ese me lo han pasado ellos."),
        ];
        let out = remove_echo(segs);
        let me: Vec<_> = out.iter().filter(|s| s.speaker == Me).collect();
        assert_eq!(me.len(), 1);
        assert_eq!(me[0].text, "Vale, el diseño ese me lo han pasado ellos.");
        assert!(me[0].start_ms > 500);
    }

    #[test]
    fn short_answers_are_kept_unless_identical() {
        let segs = vec![
            seg(Others, 0, 3000, "Si puede y ya está"),
            seg(Me, 1000, 1500, "Vale."),
            seg(Others, 5000, 5500, "¿Sí?"),
            seg(Me, 5600, 6000, "Sí."),
        ];
        let out = remove_echo(segs);
        let me: Vec<_> = out.iter().filter(|s| s.speaker == Me).map(|s| s.text.as_str()).collect();
        assert_eq!(me, vec!["Vale."]);
    }

    #[test]
    fn far_away_repetition_is_not_echo() {
        let segs = vec![seg(Others, 0, 2000, "el fondo sea continuado"), seg(Me, 30_000, 32_000, "el fondo sea continuado")];
        assert_eq!(remove_echo(segs).len(), 2);
    }

    #[test]
    fn without_system_track_nothing_changes() {
        let segs = vec![seg(Me, 0, 1000, "hola qué tal")];
        assert_eq!(remove_echo(segs.clone()), segs);
    }
}
