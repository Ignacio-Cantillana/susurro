//! Postprocesado del texto que devuelve Whisper: diccionario propio, snippets,
//! muletillas y alucinaciones típicas con audio en silencio.

use regex::{Regex, RegexBuilder};

use crate::db::{DictTerm, Snippet};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub remove_fillers: bool,
}

/// Aplica todo el pipeline en orden. Devuelve `None` si no queda nada que pegar.
pub fn process(raw: &str, dict: &[DictTerm], snippets: &[Snippet], opts: &Options) -> Option<String> {
    let mut text = normalize_whitespace(raw);
    if is_hallucination(&text) {
        return None;
    }
    if opts.remove_fillers {
        text = remove_fillers(&text);
    }
    text = apply_dictionary(&text, dict);
    text = expand_snippets(&text, snippets);
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Prompt inicial para Whisper con los términos del diccionario: le da contexto
/// para que escriba bien nombres propios y jerga.
pub fn build_prompt(dict: &[DictTerm]) -> Option<String> {
    let terms: Vec<&str> = dict.iter().map(|d| d.term.trim()).filter(|t| !t.is_empty()).collect();
    (!terms.is_empty()).then(|| format!("{}.", terms.join(", ")))
}

/// Sustituye las formas mal oídas de cada término (`sounds_like`) por la forma correcta,
/// y corrige la capitalización del propio término («n8N» → «n8n»).
pub fn apply_dictionary(text: &str, dict: &[DictTerm]) -> String {
    let mut out = text.to_string();
    for d in dict {
        let term = d.term.trim();
        if term.is_empty() {
            continue;
        }
        let variants = d
            .sounds_like
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .chain(std::iter::once(term));
        for v in variants {
            if let Some(re) = phrase_regex(v) {
                out = re.replace_all(&out, regex::NoExpand(term)).into_owned();
            }
        }
    }
    out
}

/// Expande snippets. Si todo el dictado es el disparador («Mi firma.»), se sustituye
/// entero (sin el punto que añade Whisper). Si aparece dentro de una frase, se
/// sustituye solo esa parte.
pub fn expand_snippets(text: &str, snippets: &[Snippet]) -> String {
    let bare = strip_edge_punctuation(text).to_lowercase();
    for s in snippets {
        if strip_edge_punctuation(&s.trigger).to_lowercase() == bare && !bare.is_empty() {
            return s.expansion.clone();
        }
    }
    let mut out = text.to_string();
    for s in snippets {
        if let Some(re) = phrase_regex(strip_edge_punctuation(&s.trigger)) {
            out = re.replace_all(&out, regex::NoExpand(&s.expansion)).into_owned();
        }
    }
    out
}

/// Quita muletillas sueltas («eh», «em», «mmm», «um»…) y recoloca mayúscula y espacios.
pub fn remove_fillers(text: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for w in text.split_whitespace() {
        let core = w.trim_matches(|c: char| matches!(c, ',' | '.' | '…' | '¿' | '?' | '¡' | '!'));
        if !is_filler(core) {
            words.push(w.to_string());
            continue;
        }
        // Si la muletilla cerraba la frase («…, ehh.»), el punto pasa a la palabra anterior.
        if let Some(end) = w.chars().last().filter(|c| matches!(c, '.' | '?' | '!')) {
            if let Some(prev) = words.last_mut() {
                let trimmed = prev.trim_end_matches(',').to_string();
                *prev = format!("{trimmed}{end}");
            }
        }
    }
    capitalize_first(&words.join(" "))
}

fn is_filler(word: &str) -> bool {
    let w = word.to_lowercase();
    if w.is_empty() {
        return false;
    }
    let only = |allowed: &[char]| w.chars().all(|c| allowed.contains(&c));
    matches!(w.as_str(), "eh" | "ehm" | "em" | "um" | "uh" | "ah" | "mm" | "hmm")
        || (w.len() >= 3 && only(&['m']))
        || (w.starts_with('e') && w.len() >= 3 && only(&['e', 'h', 'm']))
}

/// Frases que Whisper inventa cuando el audio es silencio o ruido.
pub fn is_hallucination(text: &str) -> bool {
    const KNOWN: &[&str] = &[
        "subtítulos realizados por la comunidad de amara.org",
        "subtitulado por la comunidad de amara.org",
        "gracias por ver el vídeo",
        "gracias por ver",
        "¡gracias por ver el vídeo!",
        "thank you for watching",
        "thanks for watching",
        "you",
        "subtítulos por la comunidad de amara.org",
    ];
    let t = strip_edge_punctuation(text).to_lowercase();
    t.is_empty() || KNOWN.iter().any(|k| strip_edge_punctuation(k) == t)
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_edge_punctuation(s: &str) -> &str {
    s.trim().trim_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '¿' | '¡' | '…') || c.is_whitespace())
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Regex que encuentra la frase como palabras completas, sin distinguir mayúsculas.
fn phrase_regex(phrase: &str) -> Option<Regex> {
    let words: Vec<String> = phrase.split_whitespace().map(regex::escape).collect();
    if words.is_empty() {
        return None;
    }
    let body = words.join(r"\s+");
    RegexBuilder::new(&format!(r"(?:^|\b){body}(?:\b|$)")).case_insensitive(true).build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(t: &str, s: &str) -> DictTerm {
        DictTerm { id: 0, term: t.into(), sounds_like: s.into() }
    }

    fn snip(t: &str, e: &str) -> Snippet {
        Snippet { id: 0, trigger: t.into(), expansion: e.into() }
    }

    #[test]
    fn dictionary_fixes_misheard_words_and_casing() {
        let dict = [term("Kubernetes", "cubernetis, kuberneti"), term("n8n", "n ocho n")];
        assert_eq!(
            apply_dictionary("Sube el flujo de N ocho N a Cubernetis y a kuberneti.", &dict),
            "Sube el flujo de n8n a Kubernetes y a Kubernetes."
        );
        assert_eq!(apply_dictionary("usa N8N", &dict), "usa n8n");
        // No toca palabras que solo contienen la variante.
        assert_eq!(apply_dictionary("cubernetisa", &dict), "cubernetisa");
    }

    #[test]
    fn prompt_lists_terms() {
        assert_eq!(build_prompt(&[term("Kubernetes", ""), term("Tauri", "")]).as_deref(), Some("Kubernetes, Tauri."));
        assert_eq!(build_prompt(&[]), None);
    }

    #[test]
    fn snippet_whole_dictation_drops_whisper_period() {
        let s = [snip("mi firma", "Un saludo,\nAlex")];
        assert_eq!(expand_snippets("Mi firma.", &s), "Un saludo,\nAlex");
    }

    #[test]
    fn snippet_inline() {
        let s = [snip("mi correo", "alex@example.com")];
        assert_eq!(expand_snippets("Escríbeme a mi correo mañana.", &s), "Escríbeme a alex@example.com mañana.");
    }

    #[test]
    fn fillers_are_removed() {
        assert_eq!(remove_fillers("Eh, quiero que, em, revises esto, ehh."), "Quiero que, revises esto.");
        assert_eq!(remove_fillers("Mmm vale"), "Vale");
        assert_eq!(remove_fillers("El tema de hemeroteca"), "El tema de hemeroteca");
    }

    #[test]
    fn hallucinations_are_dropped() {
        assert!(is_hallucination("Subtítulos realizados por la comunidad de Amara.org"));
        assert!(is_hallucination("  ¡Gracias por ver el vídeo!  "));
        assert!(is_hallucination(" . "));
        assert!(!is_hallucination("Gracias por ver el informe, lo reviso."));
    }

    #[test]
    fn full_pipeline() {
        let dict = [term("Claude", "clod")];
        let opts = Options { remove_fillers: true };
        assert_eq!(
            process("  eh, pregúntale a   clod  ", &dict, &[], &opts).as_deref(),
            Some("Pregúntale a Claude")
        );
        assert_eq!(process("Thank you for watching.", &dict, &[], &opts), None);
    }
}
