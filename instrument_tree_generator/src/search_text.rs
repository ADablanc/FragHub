//! Nettoyage des clés de l'arbre : COPIE EXACTE des fonctions de FragHub
//! (`scripts/fraghub_rust/src/normalizer/normalize_instruments_and_resolution.rs`, branche Rust_FragHub).
//! Les clés de l'arbre sont écrites comme le texte dans lequel FragHub les cherche ; si le nettoyage change
//! dans FragHub, il faut le changer ici aussi (le test `memes_regles_que_fraghub` reprend ceux de FragHub).
use once_cell::sync::Lazy;
use regex::Regex;

/// Nettoie les informations de base des instruments.
///
/// Pour un développeur Python : L'utilisation de `Lazy::new()` permet de compiler les regex
/// une seule fois au premier appel (équivalent à compiler ses regex globalement en Python),
/// évitant de tuer les performances en les recompilant à chaque passage.
fn apply_common_cleaning(mut s: String) -> String {
    // "q-tof", "it-tof", "qit-tof" -> un seul mot. (Avant : tout "-tof" était collé, et dans un type
    // "GC-EI-TOF" cela donnait "eitof" : ni l'ionisation EI ni l'analyseur TOF n'étaient plus reconnus.)
    static ANALYZER_TOF: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(q|it|qit)[- ]tof").unwrap());
    s = ANALYZER_TOF.replace_all(&s, "${1}tof").into_owned();
    s = s.replace("q-", "q");
    s = s.replace("q exactive", " qexactive ");
    s = s.replace("applied biosystems", " sciex ");
    s = s.replace(" ab ", " sciex ");
    s = s.replace("sciex", " sciex ");

    // TripleTOF (Sciex) est un Q-TOF, pas un triple quadripôle (avant : classé "qqq")
    static TRIPLE_TOF: Lazy<Regex> = Lazy::new(|| Regex::new(r"triple[- ]?tof").unwrap());
    s = TRIPLE_TOF.replace_all(&s, " qtof ").into_owned();

    static TRIPLE_QUAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"triple[- ]?quad").unwrap());
    s = TRIPLE_QUAD.replace_all(&s, " qqq ").into_owned();

    static UPLC: Lazy<Regex> = Lazy::new(|| Regex::new(r".{3} uplc .{3}").unwrap());
    s = UPLC.replace_all(&s, " ").into_owned();

    s
}

/// Nettoie un champ : minuscules, nettoyage commun PUIS tirets remplacés par des espaces.
/// (Avant, les tirets étaient remplacés en premier : "Q-TOF" devenait "q tof" et n'était jamais reconnu.)
fn clean_field(value: &str) -> String {
    apply_common_cleaning(value.to_lowercase()).replace('-', " ")
}

/// Texte de recherche : ". mots ." (caractères spéciaux retirés, espaces simples).
fn search_text(parts: &[String]) -> String {
    static CLEAN_CHARS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^-\w\s]").unwrap());
    let cleaned = CLEAN_CHARS.replace_all(&parts.join(" "), " ").into_owned();
    format!(". {} .", cleaned.split_whitespace().collect::<Vec<&str>>().join(" "))
}

/// Clé de l'arbre des instruments, écrite comme le texte de recherche (`clean_spectrum_instrument_info`) :
/// même nettoyage, caractères spéciaux retirés, espaces simples. Utilisée par le générateur de l'arbre
/// (`instrument_tree_builder.rs`), pour que chaque clé puisse être trouvée dans le texte d'un spectre qui la cite.
pub fn tree_key(value: &str) -> String {
    let text = search_text(&[clean_field(value)]);
    text.trim_start_matches(". ").trim_end_matches(" .").trim_matches('.').trim().to_string()
}

#[cfg(test)]
fn is_word_char(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' }

#[cfg(test)]
fn contains_word(text: &str, word: &str) -> bool {
    if word.is_empty() { return false; }
    let text_bytes = text.as_bytes(); // Travaille directement sur les octets ASCII
    let word_bytes = word.as_bytes();
    let word_len = word_bytes.len();

    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let actual_pos = start + pos;
        let end_pos = actual_pos + word_len;

        // Vérifie les limites du mot (boundaries)
        let is_left_boundary = actual_pos == 0 || !is_word_char(text_bytes[actual_pos - 1]);
        let is_right_boundary = end_pos == text_bytes.len() || !is_word_char(text_bytes[end_pos]);

        if is_left_boundary && is_right_boundary { return true; }
        start = actual_pos + 1;
    }
    false
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memes_regles_que_fraghub() {
        assert_eq!(tree_key("6600 TripleTOF"), "6600 qtof");
        assert_eq!(tree_key("PTR-TOF 1000"), "ptr tof 1000");
        assert_eq!(tree_key("5500+"), "5500");
        assert_eq!(tree_key("JMS-HX/HX 110A"), "jms hx hx 110a");
        assert_eq!(tree_key("Q-Exactive Orbitrap HF-X"), "qexactive orbitrap hf x");
        assert_eq!(tree_key("NOT FOUND"), "not found");
        assert_eq!(clean_field("TripleTOF 6600"), " qtof  6600");
        assert_eq!(clean_field("GC-EI-TOF"), "gc ei tof");
        assert_eq!(clean_field("LC-ESI-Q-TOF"), "lc esi qtof");
        assert_eq!(clean_field("LC-ESI-IT-TOF"), "lc esi ittof");
        // La clé est trouvée dans le texte de recherche du même nom
        for name in ["6600 TripleTOF", "PTR-TOF 1000", "5500+", "JMS-HX/HX 110A", "Q Exactive HF-X", "micrOTOF-Q"] {
            let text = search_text(&[clean_field(name)]);
            assert!(contains_word(&text, &tree_key(name)), "{name} : {text}");
        }
    }
}
