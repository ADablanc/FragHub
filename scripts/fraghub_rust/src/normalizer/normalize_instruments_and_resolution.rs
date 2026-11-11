// src/normalizer/normalize_instruments_and_resolution.rs
use std::collections::HashMap;
use regex::Regex;
use once_cell::sync::Lazy;

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
/// « QQ » (écriture MassBank du triple quadripôle) est cherché comme « qqq », l'écriture de l'arbre ; le type
/// écrit dans le fichier n'est pas modifié.
fn search_text(parts: &[String]) -> String {
    static CLEAN_CHARS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^-\w\s]").unwrap());
    let cleaned = CLEAN_CHARS.replace_all(&parts.join(" "), " ").into_owned();
    let words: Vec<&str> = cleaned.split_whitespace().map(|w| if w == "qq" { "qqq" } else { w }).collect();
    format!(". {} .", words.join(" "))
}

/// Texte dans lequel on cherche l'instrument : INSTRUMENT, INSTRUMENTTYPE, IONIZATION et les autres champs qui
/// décrivent l'instrument (clé interne INSTRUMENT_HINTS : MoNA « mass analyzer », « chromatography type »...).
/// Avant, seuls INSTRUMENT et INSTRUMENTTYPE étaient lus : l'ion source de GNPS JSON ("LC-ESI", rangé dans
/// IONIZATION) n'était pas vue, et le même instrument donnait "LC-FT" depuis le JSON et "LC-ESI-FT" depuis le MSP.
/// COMMENT n'est utilisé que si tous ces champs sont vides : sinon, une date ("2020") ou le nom
/// de la chromatographie ("Waters Acquity") pouvaient être pris pour le modèle ou la marque.
/// Dans COMMENT, les nombres décimaux sont retirés avant la recherche : les masses calculées du MSP MoNA
/// ("computed [M+Cl]-=365.526952788") donnaient le mot "365" (AB SCIEX API 365, donc "LC-QQQ") à des spectres
/// sans aucune information d'instrument. Les modèles d'appareils sont des entiers ou des mots (6530, API 3200).
/// Un COMMENT en liste "clé=valeur" "clé=valeur"... (MSP MoNA) n'est pas lu : il a déjà été découpé en champs
/// par le lecteur MSP (une vraie information d'instrument y est donc déjà dans INSTRUMENT, IONIZATION...), et la
/// recherche dans le texte entier ne trouvait que des mots au hasard ("B", "FD", "CE" -> secteur magnétique,
/// désorption de champ, électrophorèse capillaire).
fn clean_spectrum_instrument_info(metadata_dict: &HashMap<String, String>) -> String {
    let get = |k: &str| metadata_dict.get(k).map(|s| s.as_str()).unwrap_or("");
    let parts: Vec<String> = ["INSTRUMENT", "INSTRUMENTTYPE", "IONIZATION", crate::globals_vars::INSTRUMENT_HINTS_KEY]
        .iter().map(|k| clean_field(get(k))).collect();
    if parts.iter().all(|p| p.trim().is_empty()) {
        static DECIMAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-+]?(?:\d+\.\d+|\d+(?:\.\d+)?[eE][-+]?\d+)").unwrap());
        static QUOTED_FIELD: Lazy<Regex> = Lazy::new(|| Regex::new(r#""[^"=]{1,80}=[^"]*""#).unwrap());
        let comment = get("COMMENT");
        if QUOTED_FIELD.find_iter(comment).take(2).count() >= 2 {
            String::new()
        } else {
            search_text(&[clean_field(&DECIMAL.replace_all(comment, " "))])
        }
    } else {
        search_text(&parts)
    }
}

// Remplace la lenteur du regex rf"(\b|^|$){key}(\b|^|$)" de Python par une vérification native ultra-rapide
// Python utilise un moteur Regex basé sur le langage C, mais les appels répétés en Python coûtent cher.
// En Rust, analyser directement les octets (`bytes`) est immensément plus rapide.
fn is_word_char(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' }

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

/// Parcourt l'arborescence (arbre JSON de configuration) pour résoudre l'instrument.
/// La syntaxe `<'a>` décrit une "Lifetime" (durée de vie). Elle indique au compilateur que les 
/// références retournées par la fonction vivront aussi longtemps que l'arbre `tree` passé en paramètre.
/// Si aucune clé du niveau n'est dans le texte, on prend la branche "not found" (information absente),
/// comme le prévoit l'arbre. Avant, la recherche s'arrêtait : 96,6 % des spectres ne trouvaient rien.
fn search_level<'a>(tree: &'a serde_json::Value, infos: &str) -> Option<(&'a String, &'a serde_json::Value)> {
    let map = tree.as_object()?;
    for (key, next_level) in map {
        if contains_word(infos, key) { return Some((key, next_level)); }
    }
    map.get_key_value(NOT_FOUND)
}

const NOT_FOUND: &str = "not found";

/// Descend les niveaux sous un nœud (première clé trouvée, sinon la branche « not found »).
/// Retourne les clés choisies et le nœud atteint, ou None si un niveau n'a ni clé trouvée ni branche « not found ».
fn walk<'a>(start: &'a serde_json::Value, infos: &str, levels: usize) -> Option<(Vec<&'a String>, &'a serde_json::Value)> {
    let mut current = start;
    let mut keys = Vec::with_capacity(levels);
    for _ in 0..levels {
        let (key, next) = search_level(current, infos)?;
        keys.push(key);
        current = next;
    }
    Some((keys, current))
}

/// Choisit la marque puis descend les 4 autres niveaux (modèle, LC/GC, analyseur, ionisation).
/// Une seule marque citée : comme avant, c'est elle. Plusieurs marques citées (pompe, colonne ou chromatographe
/// d'une autre marque : « LC-10ADVPmicro HPLC, Shimadzu; LTQ Orbitrap, Thermo », « QE Focus, Thermo; UPLC I-Class,
/// Waters ») : on garde celle dont on trouve le modèle, sinon celle qui reconnaît le plus de niveaux, sinon la plus
/// longue clé de modèle ; à égalité, la première dans l'ordre de l'arbre. Avant, la première marque de l'arbre
/// gagnait toujours (Shimadzu, Waters) et l'instrument restait inconnu.
/// Aucune marque citée : la branche « not found » (le modèle seul peut être reconnu).
fn search_brand_and_levels<'a>(tree: &'a serde_json::Value, infos: &str) -> Option<(Vec<&'a String>, &'a serde_json::Value)> {
    let map = tree.as_object()?;
    let mut best: Option<((bool, usize, usize), Vec<&String>, &serde_json::Value)> = None;
    let mut brand_cited = false;
    for (brand, node) in map.iter().filter(|(k, _)| k.as_str() != NOT_FOUND && contains_word(infos, k)) {
        brand_cited = true;
        let Some((keys, leaf)) = walk(node, infos, 4) else { continue };
        let score = (keys[0] != NOT_FOUND, keys.iter().filter(|k| k.as_str() != NOT_FOUND).count(), keys[0].len());
        if best.as_ref().map_or(true, |(s, _, _)| score > *s) {
            let mut all = vec![brand];
            all.extend(keys);
            best = Some((score, all, leaf));
        }
    }
    if let Some((_, keys, leaf)) = best { return Some((keys, leaf)); }
    if brand_cited { return None; } // comme avant : marque trouvée mais un niveau sans issue
    let (brand, node) = map.get_key_value(NOT_FOUND)?;
    let (keys, leaf) = walk(node, infos, 4)?;
    let mut all = vec![brand];
    all.extend(keys);
    Some((all, leaf))
}

/// Éléments d'un type d'instrument que l'arbre ne doit pas faire disparaître.
const TYPE_TOKENS: &[&str] = &[
    "lc", "gc", "ce",
    "esi", "apci", "appi", "ei", "ci", "fab", "fd", "maldi", "ptr",
    "qtof", "qft", "itft", "tof", "qqq", "qq", "q", "it", "ittof", "qittof", "qit", "b", "eb", "ebeb", "ft",
];

/// Le type proposé par l'arbre contient-il tous les éléments connus du type écrit dans le fichier ?
fn keeps_type_tokens(raw_type: &str, new_type: &str) -> bool {
    let tokens = |t: &str| -> Vec<String> {
        clean_field(t).split_whitespace().map(|w| w.to_string()).filter(|w| TYPE_TOKENS.contains(&w.as_str())).collect()
    };
    let new_tokens = tokens(new_type);
    tokens(raw_type).iter().all(|t| new_tokens.contains(t))
}

/// LC/GC/CE manquant au type écrit. L'ionisation du type décide d'abord : ESI -> LC (jamais en GC), EI -> GC
/// (jamais en LC) ; sinon le LC/GC du type de l'arbre (le modèle reconnu le donne). Ex. « ESI-QTOF » d'un
/// TripleTOF 5600 : l'arbre propose « LC-ESI-TOF » (refusé, il perdrait le Q) ; on garde « ESI-QTOF » + LC.
/// L'ionisation passe avant l'arbre : le JMS DX-300 n'a pas de ligne EI dans l'Excel, l'arbre propose LC ;
/// « EI-B » doit pourtant devenir « GC-EI-B ».
/// Rien n'est ajouté si le type dit déjà LC, GC ou CE, ou s'il est vide.
fn add_missing_separation(final_type: &str, tree_type: &str) -> Option<String> {
    let words = |t: &str| -> Vec<String> { clean_field(t).split_whitespace().map(|w| w.to_string()).collect() };
    let have = words(final_type);
    if final_type.trim().is_empty() || have.iter().any(|w| matches!(w.as_str(), "lc" | "gc" | "ce")) {
        return None;
    }
    let from_ionization = if have.iter().any(|w| w == "esi") { Some("lc".to_string()) }
        else if have.iter().any(|w| w == "ei") { Some("gc".to_string()) }
        else { None };
    let from_tree = || words(tree_type).into_iter().find(|w| matches!(w.as_str(), "lc" | "gc" | "ce"));
    let separation = from_ionization.or_else(from_tree)?;
    Some(format!("{}-{}", separation.to_uppercase(), final_type.trim()))
}

/// Le spectre a-t-il été fait en injection directe (sans chromatographie) ? Mot DI dans le type ou l'ionisation,
/// mots qui le disent dans les champs d'instrument (« Flow-injection QqQ/MS », « direct infusion »), ou phrase
/// sans ambiguïté dans COMMENT (« The sample was injected by direct infusion. »).
fn is_direct_injection(metadata_dict: &HashMap<String, String>) -> bool {
    use crate::globals_vars::{DI_TYPE_PATTERN, DIRECT_INJECTION_COMMENT_PATTERN, DIRECT_INJECTION_PATTERN};
    let get = |k: &str| metadata_dict.get(k).map(|s| s.as_str()).unwrap_or("");
    DI_TYPE_PATTERN.is_match(get("INSTRUMENTTYPE")) || DI_TYPE_PATTERN.is_match(get("IONIZATION"))
        || ["INSTRUMENT", "INSTRUMENTTYPE", "IONIZATION", crate::globals_vars::INSTRUMENT_HINTS_KEY]
            .iter().any(|k| DIRECT_INJECTION_PATTERN.is_match(get(k)))
        || DIRECT_INJECTION_COMMENT_PATTERN.is_match(get("COMMENT"))
}

/// Type d'un spectre en injection directe : DI à la place du LC (« LC-ESI-QTOF » -> « DI-ESI-QTOF »), ou devant
/// (« ESI-QTOF » -> « DI-ESI-QTOF », « Flow-injection QqQ/MS » -> « DI-Flow-injection QqQ/MS »).
/// Rien si le type dit déjà DI, ou GC / EI / CE (la règle GC passe avant ; le CE est une séparation).
fn with_direct_injection(final_type: &str) -> Option<String> {
    static LC_WORD: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"(?i)(^|[^A-Za-z0-9])LC([^A-Za-z0-9]|$)").unwrap());
    let words: Vec<String> = clean_field(final_type).split_whitespace().map(|w| w.to_string()).collect();
    if words.iter().any(|w| matches!(w.as_str(), "di" | "gc" | "ei" | "ce")) { return None; }
    let t = final_type.trim();
    if t.is_empty() { return Some("DI".to_string()); }
    if LC_WORD.is_match(t) { return Some(LC_WORD.replace(t, "${1}DI${2}").into_owned()); }
    Some(format!("DI-{}", t))
}

/// Une valeur de l'arbre qui ne dit rien ("NOT FOUND") ne remplace jamais la valeur du fichier.
fn is_known(value: &str) -> bool {
    !value.is_empty() && !value.eq_ignore_ascii_case(NOT_FOUND)
}

/// Identifie avec précision le modèle d'instrument, le type d'ionisation et la résolution.
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Le dictionnaire des métadonnées du spectre.
/// * `context` (&super::NormalizerContext) : Contexte contenant l'arbre de décision JSON des instruments.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire mis à jour avec INSTRUMENT, INSTRUMENTTYPE, RESOLUTION.
pub fn normalize_instruments_and_resolution(mut metadata_dict: HashMap<String, String>, context: &super::NormalizerContext) -> HashMap<String, String> {
    let infos = clean_spectrum_instrument_info(&metadata_dict);
    let direct_injection = is_direct_injection(&metadata_dict);
    // Clé interne : lue une seule fois, ici
    metadata_dict.remove(crate::globals_vars::INSTRUMENT_HINTS_KEY);

    // On descend les 5 niveaux (Marque, Modèle, Type Spectre, Type Instrument, Ionisation)
    let Some((keys, current_level)) = search_brand_and_levels(&context.instrument_tree, &infos) else {
        return metadata_dict; // Niveau sans clé correspondante ni branche "not found" : dico intact
    };
    let tree_levels: Vec<String> = keys.into_iter().cloned().collect();

    if tree_levels.len() == 5 {
        metadata_dict.insert("TREE_MARQUE".to_string(), tree_levels[0].clone());
        metadata_dict.insert("TREE_MODELE".to_string(), tree_levels[1].clone());
        metadata_dict.insert("TREE_SPECTYPE".to_string(), tree_levels[2].clone());
        metadata_dict.insert("TREE_INSTYPE".to_string(), tree_levels[3].clone());
        metadata_dict.insert("TREE_IONI".to_string(), tree_levels[4].clone());
    }

    // Niveau 6 : Résolution
    // L'arbre complète les champs ; il ne remplace jamais une valeur du fichier par une valeur moins précise.
    if let Some(res_level) = current_level.as_object() {
        let resolution = if res_level.contains_key("high") { "high" }
        else if res_level.contains_key("low") { "low" }
        else { NOT_FOUND };

        // Chaînage de méthodes sécurisé via `and_then` pour naviguer dans l'objet JSON.
        if let Some(solution_str) = res_level.get(resolution).and_then(|r| r.get("SOLUTION")).and_then(|s| s.as_str()) {
            let parts: Vec<&str> = solution_str.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3 {
                let raw_instrument = metadata_dict.get("INSTRUMENT").cloned().unwrap_or_default();
                let raw_type = metadata_dict.get("INSTRUMENTTYPE").cloned().unwrap_or_default();
                let raw_res = metadata_dict.get("RESOLUTION").cloned().unwrap_or_default().to_lowercase();

                // INSTRUMENT : une solution générique ("Agilent instrument") ne remplace pas un nom écrit
                // (ex. "Pegasus III TOF-MS system, Leco; GC 6890, Agilent" ou "Bruker timsTOF Pro 2").
                let generic = parts[0].to_lowercase().ends_with(" instrument");
                if is_known(parts[0]) && (raw_instrument.trim().is_empty() || !generic) {
                    metadata_dict.insert("INSTRUMENT".to_string(), parts[0].to_string());
                }

                // INSTRUMENTTYPE : remplacé seulement si le type de l'arbre garde tous les éléments connus
                // du type écrit (LC/GC, ionisation, analyseur). Sinon "LC-ESI-QQ" devenait "LC-ESI",
                // "GC-EI-TOF" devenait "GC", "EI-B" perdait son EI (utilisé pour reconnaître le GC).
                if is_known(parts[1]) && keeps_type_tokens(&raw_type, parts[1]) {
                    metadata_dict.insert("INSTRUMENTTYPE".to_string(), parts[1].to_string());
                }
                // Injection directe : DI (à la place du LC) ; sinon LC/GC/CE absent du type gardé complété
                // (depuis l'arbre, ou LC pour l'ESI, GC pour l'EI)
                let kept = metadata_dict.get("INSTRUMENTTYPE").cloned().unwrap_or_default();
                let separated = if direct_injection { with_direct_injection(&kept) }
                    else { add_missing_separation(&kept, if is_known(parts[1]) { parts[1] } else { "" }) };
                if let Some(t) = separated {
                    metadata_dict.insert("INSTRUMENTTYPE".to_string(), t);
                }

                // RESOLUTION : une résolution écrite comme nominale / basse n'est pas remplacée
                let says_low = raw_res.contains("nominal") || raw_res.contains("low") || raw_res.contains("unit");
                if is_known(parts[2]) && !says_low {
                    metadata_dict.insert("RESOLUTION".to_string(), parts[2].to_string());
                }
            }
        }
    }

    // IONIZATION : celle du fichier si elle est connue ; sinon celle du niveau « ionisation » de l'arbre ;
    // sinon celle écrite dans le type final (ex. "LC-ESI-QTOF" -> ESI).
    // (Avant : 2e morceau du type de l'arbre, faux pour les types en deux parties : "ESI-QFT" donnait "QFT".)
    let current_ionization = metadata_dict.get("IONIZATION").cloned().unwrap_or_default();
    if current_ionization.trim().is_empty() {
        let from_type = metadata_dict.get("INSTRUMENTTYPE")
            .and_then(|t| crate::globals_vars::IONIZATION_MODE_PATTERN.captures(t))
            .and_then(|c| c.get(1)).map(|m| m.as_str().to_uppercase());
        let ionization = if is_known(&tree_levels[4]) { Some(tree_levels[4].to_uppercase()) } else { from_type };
        if let Some(ion) = ionization {
            metadata_dict.insert("IONIZATION".to_string(), if ion == "ACPI" { "APCI".to_string() } else { ion });
        }
    }

    metadata_dict
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait du vrai arbre (datas/instruments_tree.json), mêmes clés et mêmes solutions.
    fn context() -> super::super::NormalizerContext {
        let tree: serde_json::Value = serde_json::from_str(r#"{
          "not found": {
            "orbitrap": {"not found": {"not found": {"not found": {"high": {"SOLUTION": "Thermo Fisher Scientific Orbitrap, LC-FT, HIGH"}}}}},
            "not found": {
              "not found": {"qtof": {"not found": {"high": {"SOLUTION": "NOT FOUND, QTOF, HIGH"}}}},
              "lc": {"qtof": {"esi": {"high": {"SOLUTION": "NOT FOUND, LC-ESI-QTOF, HIGH"}}}}
            }
          },
          "hitachi": {"not found": {"not found": {"b": {"not found": {"low": {"SOLUTION": "Hitachi instrument, B, LOW"}}}}}},
          "thermo": {"qexactive": {"not found": {"qft": {"esi": {"high": {"SOLUTION": "Thermo Fisher Scientific Q-Exactive, ESI-QFT, HIGH"}}}}}},
          "bruker": {"not found": {"not found": {"qtof": {"not found": {"high": {"SOLUTION": "Bruker instrument, QTOF, HIGH"}}}}}}
        }"#).unwrap();
        super::super::NormalizerContext {
            adduct_pos: HashMap::new(), adduct_neg: HashMap::new(),
            adduct_massdiff_pos: HashMap::new(), adduct_massdiff_neg: HashMap::new(),
            instrument_tree: tree,
        }
    }

    fn run(fields: &[(&str, &str)]) -> HashMap<String, String> {
        let m: HashMap<String, String> = fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        normalize_instruments_and_resolution(m, &context())
    }

    #[test]
    fn branche_not_found_quand_l_information_manque() {
        // GNPS : "Orbitrap" seul, ni marque ni type (avant : aucune correspondance)
        let m = run(&[("INSTRUMENT", "Orbitrap"), ("INSTRUMENTTYPE", "")]);
        assert_eq!(m["INSTRUMENT"], "Thermo Fisher Scientific Orbitrap");
        assert_eq!(m["INSTRUMENTTYPE"], "LC-FT");
        assert_eq!(m["RESOLUTION"], "HIGH");
        // La solution "NOT FOUND" ne remplace pas la valeur du fichier
        let m = run(&[("INSTRUMENT", ""), ("INSTRUMENTTYPE", "qtof")]);
        assert_eq!(m["INSTRUMENT"], "");
        assert_eq!(m["INSTRUMENTTYPE"], "QTOF");
    }

    #[test]
    fn ionisation_et_autres_champs_lus_pour_l_instrument() {
        // GNPS JSON : Instrument "Orbitrap", Ion_Source "LC-ESI" (rangé dans IONIZATION), pas de type
        let tree: serde_json::Value = serde_json::from_str(r#"{"not found": {"orbitrap": {"lc": {"not found": {
            "esi": {"high": {"SOLUTION": "Thermo Fisher Scientific Orbitrap, LC-ESI-FT, HIGH"}},
            "not found": {"high": {"SOLUTION": "Thermo Fisher Scientific Orbitrap, LC-FT, HIGH"}}}}}}}"#).unwrap();
        let ctx = super::super::NormalizerContext {
            adduct_pos: HashMap::new(), adduct_neg: HashMap::new(),
            adduct_massdiff_pos: HashMap::new(), adduct_massdiff_neg: HashMap::new(), instrument_tree: tree,
        };
        let run2 = |f: &[(&str, &str)]| normalize_instruments_and_resolution(f.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), &ctx);
        let m = run2(&[("INSTRUMENT", "Orbitrap"), ("INSTRUMENTTYPE", ""), ("IONIZATION", "LC-ESI")]);
        assert_eq!(m["INSTRUMENTTYPE"], "LC-ESI-FT");
        // Indice venu d'un autre champ (clé interne), retiré ensuite
        let m = run2(&[("INSTRUMENT", "Orbitrap"), ("INSTRUMENT_HINTS", "Liquid Chromatography (LC) ESI")]);
        assert_eq!(m["INSTRUMENTTYPE"], "LC-ESI-FT");
        assert!(!m.contains_key("INSTRUMENT_HINTS"));
    }

    #[test]
    fn q_tof_avec_tiret_reconnu() {
        // MoNA : "Bruker timsTOF Pro 2" / "Q-TOF" (avant : "q tof", jamais reconnu)
        let m = run(&[("INSTRUMENT", "Bruker timsTOF Pro 2"), ("INSTRUMENTTYPE", "Q-TOF")]);
        assert_eq!(m["INSTRUMENTTYPE"], "QTOF");
        let m = run(&[("INSTRUMENTTYPE", "LC-ESI-Q-TOF")]);
        assert_eq!(m["INSTRUMENTTYPE"], "LC-ESI-QTOF");
        assert_eq!(m["IONIZATION"], "ESI");
    }

    #[test]
    fn mention_ei_jamais_effacee() {
        // Hitachi "EI-B" : la solution de l'arbre dit "B" ; on garde le EI (et le GC est ajouté : EI -> GC)
        let m = run(&[("INSTRUMENT", "HITACHI M-80B"), ("INSTRUMENTTYPE", "EI-B")]);
        assert_eq!(m["INSTRUMENTTYPE"], "GC-EI-B");
        assert_eq!(m["INSTRUMENT"], "HITACHI M-80B");
        assert_eq!(m["RESOLUTION"], "LOW");
    }

    #[test]
    fn jamais_moins_precis_que_le_fichier() {
        // Type écrit plus complet que la solution de l'arbre : gardé
        assert!(!keeps_type_tokens("LC-ESI-QQ", "LC-ESI"));
        assert!(!keeps_type_tokens("GC-EI-TOF", "GC"));
        assert!(!keeps_type_tokens("LC-ESI-QFT", "LC-ESI-ITFT"));
        assert!(!keeps_type_tokens("EI-B", "GC-B"));
        // Type écrit sans élément connu, ou contenu dans la solution : remplacé
        assert!(keeps_type_tokens("Orbitrap", "LC-FT"));
        assert!(keeps_type_tokens("", "QTOF"));
        assert!(keeps_type_tokens("Q-TOF", "QTOF"));
        assert!(keeps_type_tokens("LC-ESI-MS", "LC-ESI-FT"));
        // Nom d'instrument écrit : la solution générique ne le remplace pas
        let m = run(&[("INSTRUMENT", "Bruker timsTOF Pro 2"), ("INSTRUMENTTYPE", "Q-TOF")]);
        assert_eq!(m["INSTRUMENT"], "Bruker timsTOF Pro 2");
        // Résolution nominale : gardée
        let m = run(&[("INSTRUMENT", "Orbitrap"), ("RESOLUTION", "nominal")]);
        assert_eq!(m["RESOLUTION"], "nominal");
    }

    #[test]
    fn ionisation_prise_au_niveau_ionisation() {
        // Avant : 2e morceau du type, "ESI-QFT" donnait IONIZATION = "QFT"
        let m = run(&[("INSTRUMENT", "Thermo Q Exactive HF"), ("INSTRUMENTTYPE", "ESI-QFT"), ("IONIZATION", "ESI")]);
        assert_eq!(m["INSTRUMENTTYPE"], "LC-ESI-QFT"); // l'ESI est toujours en LC
        assert_eq!(m["IONIZATION"], "ESI");
        // IONIZATION du fichier connue : jamais remplacée
        let m = run(&[("INSTRUMENT", "Orbitrap"), ("IONIZATION", "APCI")]);
        assert_eq!(m["IONIZATION"], "APCI");
        // IONIZATION vide : prise dans le type final
        let m = run(&[("INSTRUMENTTYPE", "LC-ESI-Q-TOF"), ("IONIZATION", "")]);
        assert_eq!(m["IONIZATION"], "ESI");
    }

    #[test]
    fn commentaire_seulement_si_instrument_et_type_vides() {
        let text = clean_spectrum_instrument_info(&[("INSTRUMENT", "Orbitrap"), ("COMMENT", "acquired 2020 on Waters Acquity")]
            .iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        assert_eq!(text, ". orbitrap .");
        let text = clean_spectrum_instrument_info(&[("COMMENT", "instrument=Orbitrap")]
            .iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        assert_eq!(text, ". instrument orbitrap .");
    }

    #[test]
    fn commentaire_en_liste_de_champs_pas_relu_pour_l_instrument() {
        // MSP MoNA : liste "clé=valeur" déjà découpée en champs ; des lettres isolées ne donnent plus "B" ou "FD"
        let comment = r#""SMILES=CC" "isomer=B" "comment=FD 30 CE" "MoNA Rating=3.5""#;
        let m: HashMap<String, String> = [("COMMENT", comment)].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(clean_spectrum_instrument_info(&m), "");
        // Commentaire en texte libre : toujours lu
        let m: HashMap<String, String> = [("COMMENT", "measured on a Q Exactive, ESI positive")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let text = clean_spectrum_instrument_info(&m);
        assert!(contains_word(&text, "qexactive") && contains_word(&text, "esi"), "{text}");
    }

    #[test]
    fn masses_du_commentaire_mona_pas_prises_pour_un_modele() {
        // Extrait réel du MSP MoNA (1001249) : aucune information d'instrument, mais des masses calculées
        // (sans guillemets ici : en liste "clé=valeur", le commentaire n'est plus lu du tout)
        let comment = r#"computed [M+Cl]-=365.526952788; computed [2M+H]+=2000.0123; computed mass error=-5.040000132794376E-7; MoNA Rating=3.333333333333333; origin=natural_products_neg.msp"#;
        let text = clean_spectrum_instrument_info(&[("COMMENT", comment)].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        assert!(!contains_word(&text, "365"), "{text}");
        assert!(!contains_word(&text, "2000"), "{text}");
        assert!(!contains_word(&text, "333333333333333"), "{text}");
        // Un modèle écrit en entier dans le commentaire reste visible
        let text = clean_spectrum_instrument_info(&[("COMMENT", "instrument=API 3200; rating=4.5")]
            .iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        assert!(contains_word(&text, "3200") && contains_word(&text, "api"), "{text}");
    }

    #[test]
    fn plusieurs_marques_citees_celle_dont_on_trouve_le_modele() {
        let tree: serde_json::Value = serde_json::from_str(r#"{
          "not found": {"not found": {"lc": {"not found": {"esi": {"not found": {"SOLUTION": "NOT FOUND, LC-ESI, NOT FOUND"}}}}}},
          "shimadzu": {"not found": {"lc": {"not found": {"esi": {"not found": {"SOLUTION": "Shimadzu instrument, LC-ESI, NOT FOUND"}}}}}},
          "waters": {"not found": {"lc": {"not found": {"esi": {"not found": {"SOLUTION": "Waters instrument, LC-ESI, NOT FOUND"}}}}}},
          "thermo": {
            "ltq orbitrap": {"lc": {"itft": {"esi": {"high": {"SOLUTION": "Thermo Fisher Scientific LTQ Orbitrap, LC-ESI-ITFT, HIGH"}}}}},
            "not found": {"lc": {"qft": {"esi": {"high": {"SOLUTION": "Thermo Fisher Scientific instrument, LC-ESI-QFT, HIGH"}}},
                                 "not found": {"esi": {"not found": {"SOLUTION": "Thermo Fisher Scientific instrument, LC-ESI, NOT FOUND"}}}}}
          }
        }"#).unwrap();
        let ctx = super::super::NormalizerContext {
            adduct_pos: HashMap::new(), adduct_neg: HashMap::new(),
            adduct_massdiff_pos: HashMap::new(), adduct_massdiff_neg: HashMap::new(), instrument_tree: tree,
        };
        let run2 = |f: &[(&str, &str)]| normalize_instruments_and_resolution(f.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), &ctx);
        // Pompe Shimadzu + spectromètre Thermo : le modèle est chez Thermo
        let m = run2(&[("INSTRUMENT", "LC-10ADVPmicro HPLC, Shimadzu; LTQ Orbitrap, Thermo Scientific"), ("INSTRUMENTTYPE", "LC-ESI-ITFT")]);
        assert_eq!(m["TREE_MARQUE"], "thermo");
        assert_eq!(m["RESOLUTION"], "HIGH");
        // Modèle inconnu partout : la marque qui reconnaît le plus de niveaux (Thermo connaît le QFT, Waters non)
        let m = run2(&[("INSTRUMENT", "QE Focus, Thermo Scientfic; UPLC I-Class, Waters"), ("INSTRUMENTTYPE", "LC-ESI-QFT")]);
        assert_eq!(m["TREE_MARQUE"], "thermo");
        assert_eq!(m["RESOLUTION"], "HIGH");
        // Une seule marque : inchangé
        let m = run2(&[("INSTRUMENT", "Waters Xevo"), ("INSTRUMENTTYPE", "LC-ESI")]);
        assert_eq!(m["TREE_MARQUE"], "waters");
        // Aucune marque : branche « not found »
        let m = run2(&[("INSTRUMENTTYPE", "LC-ESI")]);
        assert_eq!(m["TREE_MARQUE"], "not found");
    }

    #[test]
    fn lc_ajoute_quand_le_type_garde_n_a_pas_de_separation() {
        // Depuis l'arbre : le type de l'arbre est refusé (perd le Q) mais son LC est repris
        assert_eq!(add_missing_separation("ESI-QTOF", "LC-ESI-TOF").as_deref(), Some("LC-ESI-QTOF"));
        assert_eq!(add_missing_separation("EI-B", "GC-EI-B").as_deref(), Some("GC-EI-B"));
        // Sans information de l'arbre : l'ESI est toujours en LC
        assert_eq!(add_missing_separation("ESI-QFT", "").as_deref(), Some("LC-ESI-QFT"));
        // Rien à ajouter ou rien de sûr
        assert_eq!(add_missing_separation("LC-ESI-QTOF", "GC-EI-TOF"), None);
        assert_eq!(add_missing_separation("CE-ESI-TOF", ""), None);
        assert_eq!(add_missing_separation("APCI-QTOF", ""), None); // l'APCI existe en LC et en GC
        // L'ionisation passe avant l'arbre : EI toujours en GC, même si l'arbre dit LC (JMS DX-300)
        assert_eq!(add_missing_separation("EI-B", "LC-B").as_deref(), Some("GC-EI-B"));
        assert_eq!(add_missing_separation("EI-B", "").as_deref(), Some("GC-EI-B"));
        assert_eq!(add_missing_separation("ESI-QTOF", "GC-QTOF").as_deref(), Some("LC-ESI-QTOF"));
        // Ionisation qui ne dit rien de sûr : l'arbre
        assert_eq!(add_missing_separation("FAB-B", "LC-FAB-B").as_deref(), Some("LC-FAB-B"));
        assert_eq!(add_missing_separation("CI-B", "").as_deref(), None);
        assert_eq!(add_missing_separation("", "LC-ESI-QTOF"), None);
    }

    #[test]
    fn injection_directe_en_di() {
        assert_eq!(with_direct_injection("LC-ESI-QTOF").as_deref(), Some("DI-ESI-QTOF"));
        assert_eq!(with_direct_injection("ESI-QTOF").as_deref(), Some("DI-ESI-QTOF"));
        assert_eq!(with_direct_injection("").as_deref(), Some("DI"));
        assert_eq!(with_direct_injection("DI-ESI-qTof"), None);
        assert_eq!(with_direct_injection("GC-EI-TOF"), None);
        assert_eq!(with_direct_injection("EI-B"), None);
        assert_eq!(with_direct_injection("CE-ESI-TOF"), None);
        // Détection : type, champs d'instrument, phrase du commentaire ; rien sinon
        let m = |f: &[(&str, &str)]| f.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>();
        assert!(is_direct_injection(&m(&[("INSTRUMENTTYPE", "DI-ESI-qTof")])));
        assert!(is_direct_injection(&m(&[("INSTRUMENT", "Flow-injection QqQ/MS")])));
        assert!(is_direct_injection(&m(&[("COMMENT", "origin=MassBank; The sample was injected by direct infusion.")])));
        assert!(!is_direct_injection(&m(&[("INSTRUMENTTYPE", "LC-ESI-QTOF"), ("COMMENT", "infusion study")])));
        // Bout à bout : LC du fichier remplacé par DI quand le commentaire dit injection directe
        let r = run(&[("INSTRUMENT", "X500R QTOF (AB Sciex LLC, MA, USA)"), ("INSTRUMENTTYPE", "ESI-QTOF"),
                      ("COMMENT", "The sample was injected by direct infusion.")]);
        assert!(r["INSTRUMENTTYPE"].starts_with("DI-"), "{}", r["INSTRUMENTTYPE"]);
        let r = run(&[("INSTRUMENT", "X500R QTOF"), ("INSTRUMENTTYPE", "ESI-QTOF")]);
        assert!(r["INSTRUMENTTYPE"].starts_with("LC-"), "{}", r["INSTRUMENTTYPE"]);
    }

    #[test]
    fn qq_cherche_comme_qqq() {
        assert_eq!(search_text(&[clean_field("LC-ESI-QQ")]), ". lc esi qqq .");
        assert_eq!(search_text(&[clean_field("Xevo TQ")]), ". xevo tq ."); // seul le mot QQ change
    }

    #[test]
    fn tripletof_est_un_qtof() {
        assert!(clean_field("TripleTOF 6600").contains("qtof"));
        assert!(clean_field("Triple Quad 5500").contains("qqq"));
        assert_eq!(clean_field("GC-EI-TOF"), "gc ei tof");
        assert_eq!(clean_field("LC-ESI-Q-TOF"), "lc esi qtof");
        assert_eq!(clean_field("LC-ESI-IT-TOF"), "lc esi ittof");
    }
}

