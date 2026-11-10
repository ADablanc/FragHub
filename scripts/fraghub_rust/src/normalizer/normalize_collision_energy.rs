// src/normalizer/normalize_collision_energy.rs
use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;

// Uniformise l'écriture de l'énergie de collision, SANS conversion d'unité :
//   - unité gardée telle qu'elle est (eV, V) ; pourcentages, NCE et « (nominal) » écrits « NCE » ;
//   - décimales inutiles retirées (20.0 -> 20) ;
//   - valeur simple : « nombre unité » (« 35 eV ») ; sans unité : le nombre seul ;
//   - plusieurs valeurs : séparées par « / », précédées de « Ramp » ou « Stepped » seulement quand la source le
//     dit ou que l'écriture le rend évident, sinon liste neutre (« 20/30/60 ») ;
//   - méthode de fragmentation écrite avec l'énergie (« 35 CID », « HCD (NCE 20-30-40%) », « 65HCD ») : mise dans
//     FRAGMENTATIONMETHODE si ce champ est vide, retirée de l'énergie.
// Une écriture qui n'entre pas dans ces cas (texte, « 3 kV », « 10-30V+20V », « low », « CE0 »...) est laissée
// telle quelle. L'analyseur écrit après l'énergie (« 35 eV FT-MS ») est retiré avant (analyzer_from_collision_energy).

/// Sigles de méthodes de fragmentation.
const METHODS: &str = r"HCD|CID|CAD|ETD|ECD|EAD|EID|UVPD|IRMPD|PQD|EThcD|ETciD";
/// Sigle isolé : ni lettre juste avant, ni lettre juste après (« 35 CID », « 65HCD », « HCD; Lumos », « LC-ESI-HCD »,
/// « origin=HCD natural product library », mais pas « acid », « Hcdx »). Un chiffre peut être collé.
static METHOD_TOKEN: Lazy<Regex> = Lazy::new(|| Regex::new(&format!(r"(?:^|[^A-Za-z])({METHODS})(?:[^A-Za-z]|$)")).unwrap());
static METHOD_TOKEN_ANY_CASE: Lazy<Regex> = Lazy::new(|| Regex::new(&format!(r"(?i)(?:^|[^a-z])({METHODS})(?:[^a-z]|$)")).unwrap());
/// Identifiant PubChem juste après « CID » (« CID 638 », « CID:102004382 », « cid=71485 ») : pas une méthode.
static PUBCHEM_AFTER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*[:=#]?\s*\d{3,}").unwrap());
/// Fragmentation dans la source (« in source CID ») : écrite « in-source CID ».
static IN_SOURCE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bin[\s_-]*source[\s_-]*(?:CID|fragmentation)\b").unwrap());
/// Méthode et énergie collées dans un nom GNPS : "Catechin_HCD40_170906", "…pyrrole_HCD50".
static NAME_METHOD_ENERGY: Lazy<Regex> = Lazy::new(|| Regex::new(r"[_\s](HCD|CID)(\d{1,3}(?:\.\d+)?)(?:[_\s]|$)").unwrap());
/// Origine écrite dans le commentaire (MS-DIAL « DB#=…; origin=HCD natural product library, … »,
/// MoNA MSP « "origin=uvpd.msp" »).
static COMMENT_ORIGIN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)\borigin\s*=\s*([^;"]*)"#).unwrap());
/// Nom de bibliothèque ou de fichier qui commence par un sigle (« HCD natural product library »,
/// « HCD_natural_product_library », « UVPD Library », « uvpd.msp », « hcd_lib.msp », « ead_pfp.msp »).
static LIBRARY_PIECE: Lazy<Regex> = Lazy::new(|| Regex::new(&format!(
    r"(?i)^({METHODS})(?:[\s_.-][^|;,]*)?(?:lib|library|\.msp|\.mgf|\.txt|\.mzml|\.mzxml)\b")).unwrap());
/// Mots d'une méthode écrite en toutes lettres (« collision-induced dissociation », « in-source fragmentation »).
static METHOD_WORDS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)dissociation|fragmentation|activation|in[\s_-]*source").unwrap());
/// Analyseur écrit après l'énergie (MassBank KWR « 35 eV FT-MS », « 35 eV FT-MS II », « 35 eV IT-MS »).
static ENERGY_ANALYZER: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(.*\d.*?)\s+((?:FT|IT)-?MS(?:\s+II)?)\s*$").unwrap());

/// Écriture unique d'un sigle ("cid" -> "CID", "ethcd" -> "EThcD").
fn canonical_method(m: &str) -> String {
    METHODS.split('|').find(|c| c.eq_ignore_ascii_case(m)).map(|c| c.to_string()).unwrap_or_else(|| m.to_uppercase())
}

/// Méthodes distinctes trouvées comme sigles isolés dans un texte, en écartant les identifiants PubChem.
fn method_tokens(text: &str, rx: &Regex) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for c in rx.captures_iter(text) {
        let m = c.get(1).unwrap();
        let after = &text[m.end()..];
        let before = &text[..m.start()];
        let before_low = before[before.len().saturating_sub(12)..].to_lowercase();
        if m.as_str().eq_ignore_ascii_case("CID") && (PUBCHEM_AFTER.is_match(after) || before_low.contains("pubchem")) {
            continue;
        }
        let canon = canonical_method(m.as_str());
        if !found.contains(&canon) {
            found.push(canon);
        }
    }
    found
}

/// Mots retirés une fois leur sens noté (rampe, paliers, NCE), dans cet ordre.
const RAMP_WORDS: [&str; 3] = ["ramped collision energies of", "ramped", "ramp"];
const NCE_WORDS: [&str; 6] = ["(nominal)", "nominal", "(nce)", "nce", "%", "="];

#[derive(Debug, PartialEq, Clone, Copy)]
enum Unit { None, Ev, V, Nce }

#[derive(Debug, PartialEq)]
enum Kind { Single, Ramp, Stepped, Neutral }

/// Nombre sans décimales inutiles : "20.0" -> "20", "17.60" -> "17.6", "-1.0" -> "-1".
fn tidy_number(n: &str) -> String {
    if let Some((int, dec)) = n.split_once('.') {
        let dec = dec.trim_end_matches('0');
        let int = if int.is_empty() || int == "-" { format!("{int}0") } else { int.to_string() };
        if dec.is_empty() { int } else { format!("{int}.{dec}") }
    } else {
        n.to_string()
    }
}

/// Découpe "20-40", "-10, -35, -60", "10->20", "20 to 40", "[20.0, 30.0]" en nombres et séparateurs.
/// Un « - » est un signe s'il est en tête ou suit un séparateur (virgule, espace, « / », « [ ») ; il est un
/// séparateur s'il suit un chiffre. Renvoie None si autre chose qu'un nombre ou un séparateur reste.
fn tokenize(s: &str) -> Option<(Vec<String>, Vec<String>)> {
    let chars: Vec<char> = s.chars().collect();
    let mut nums: Vec<String> = Vec::new();
    let mut seps: Vec<String> = Vec::new();
    let mut sep = String::new();
    let mut i = 0;
    let mut last_was_number = false;
    while i < chars.len() {
        let c = chars[i];
        let next_digit = i + 1 < chars.len() && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.');
        let starts_number = c.is_ascii_digit()
            || (c == '-' && next_digit && !last_was_number_before(&sep, last_was_number));
        if starts_number {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if !nums.is_empty() {
                seps.push(sep.trim().to_string());
            }
            sep.clear();
            nums.push(chars[start..i].iter().collect());
            last_was_number = true;
            continue;
        }
        if c.is_whitespace() || ",/-–>[]_".contains(c) {
            sep.push(c);
            if !c.is_whitespace() {
                last_was_number = false;
            }
            i += 1;
            continue;
        }
        if s[char_offset(s, i)..].starts_with("to") {
            sep.push_str("->");
            last_was_number = false;
            i += 2;
            continue;
        }
        return None;
    }
    if nums.is_empty() { None } else { Some((nums, seps)) }
}

/// Un « - » juste après un nombre (sans autre séparateur entre les deux) est un séparateur, pas un signe.
fn last_was_number_before(sep: &str, last_was_number: bool) -> bool {
    last_was_number && sep.trim().is_empty()
}

fn char_offset(s: &str, char_index: usize) -> usize {
    s.char_indices().nth(char_index).map(|(o, _)| o).unwrap_or(s.len())
}

/// Énergie uniformisée et méthode trouvée, ou None si l'écriture n'est pas reconnue (laissée telle quelle).
fn tidy(value: &str) -> Option<(String, Option<String>)> {
    let original = value.trim();
    let mut t = original.to_lowercase();
    if t.is_empty() || t == "ce0" {
        return None;
    }
    // "CE75" : « CE » veut seulement dire énergie de collision ("CE0" est laissé tel quel plus haut)
    if let Some(rest) = t.strip_prefix("ce") {
        if rest.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
            t = rest.to_string();
        }
    }
    // Méthode de fragmentation écrite avec l'énergie (une seule)
    let methods = method_tokens(&t, &METHOD_TOKEN_ANY_CASE);
    let method = match methods.as_slice() {
        [] => None,
        [m] => Some(m.clone()),
        _ => return None,
    };
    if method.is_some() {
        t = Regex::new(&format!("(?i){}", METHODS)).unwrap().replace_all(&t, " ").to_string();
    }
    let mut ramp = false;
    for w in RAMP_WORDS {
        if t.contains(w) {
            ramp = true;
            t = t.replace(w, " ");
        }
    }
    let stepped = t.contains("stepped");
    t = t.replace("stepped", " ");
    let mut units: Vec<Unit> = Vec::new();
    if NCE_WORDS.iter().any(|w| t.contains(w)) {
        units.push(Unit::Nce);
        for w in NCE_WORDS {
            t = t.replace(w, " ");
        }
    }
    if t.contains("ev") {
        units.push(Unit::Ev);
        t = t.replace("ev", " ");
    }
    if t.contains('v') {
        units.push(Unit::V);
        t = t.replace('v', " ");
    }
    t = t.replace(['(', ')'], " ");
    if units.len() > 1 {
        return None;
    }
    let unit = units.first().copied().unwrap_or(Unit::None);
    let (mut nums, seps) = tokenize(&t)?;
    let kind = if ramp {
        Kind::Ramp
    } else if stepped {
        Kind::Stepped
    } else if nums.len() == 1 {
        Kind::Single
    } else if seps.iter().all(|s| s == "->") {
        Kind::Ramp
    } else if seps.iter().all(|s| s == "-" || s == "–") {
        if nums.len() == 2 { Kind::Ramp } else { Kind::Stepped }
    } else {
        Kind::Neutral
    };
    // GNPS « CollisionEnergy:102040 » : énergies de deux chiffres collées (10, 20, 40)
    if kind == Kind::Single && unit == Unit::None {
        let n = &nums[0];
        if n.len() >= 4 && n.len() % 2 == 0 && n.chars().all(|c| c.is_ascii_digit()) {
            let parts: Vec<String> = (0..n.len()).step_by(2).map(|i| n[i..i + 2].to_string()).collect();
            if parts.iter().all(|p| p.parse::<u32>().map_or(false, |v| v >= 10)) {
                return Some((parts.join("/"), method));
            }
        }
    }
    nums = nums.iter().map(|n| tidy_number(n)).collect();
    let body = nums.join("/");
    let prefix = match kind { Kind::Ramp => "Ramp ", Kind::Stepped => "Stepped ", _ => "" };
    let suffix = match unit { Unit::None => "", Unit::Ev => " eV", Unit::V => " V", Unit::Nce => " NCE" };
    Some((format!("{prefix}{body}{suffix}"), method))
}

/// Méthode de fragmentation écrite ailleurs que dans son champ, mise dans FRAGMENTATIONMETHODE s'il est vide :
/// - sigle isolé dans l'instrument ou son type (GNPS « HCD; Lumos », « LC-ESI-CID »), dans les indices d'instrument,
///   dans l'origine / la bibliothèque (MS-DIAL « origin=HCD natural product library », MoNA « UVPD on Thermo IQX »,
///   « …_dG-ONE_HCD_60.txt ») ; « in source CID » -> « in-source CID » ; rien si plusieurs méthodes différentes ;
/// - méthode et énergie collées dans le nom (GNPS « Catechin_HCD40_170906 ») : méthode, et énergie si elle est vide ;
///   rien si le nom contient plusieurs méthodes (« …_CID25_MS3-465_HCD100_… »).
/// Les sigles sont pris en majuscules seulement, isolés, et jamais un identifiant PubChem (« CID 638 »).
/// Une méthode déjà écrite en minuscules (« cid ») est mise sous sa forme unique (« CID »).
/// Une valeur de FRAGMENTATIONMETHODE qui n'est pas une méthode (MoNA NIST-Glycan « IT/ion trap »,
/// « IT-FT/ion trap with FTMS » : des analyseurs) est vidée, puis la méthode est cherchée ailleurs.
/// Si le texte cite plusieurs méthodes (« HCD natural product library, acquired in adjunct with UVPD library »),
/// ou seulement en minuscules (« uvpd.msp »), seule celle qui commence un nom de bibliothèque ou de fichier est
/// prise, si elle est unique.
/// À appeler avant normalize_predicted et la normalisation des instruments, qui retirent les indices.
pub fn fragmentation_method_from_text(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let get = |m: &HashMap<String, String>, k: &str| m.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
    let empty = |v: &str| crate::globals_vars::EMPTY_PATTERN.is_match(v);
    let current = get(&metadata_dict, "FRAGMENTATIONMETHODE");
    if !empty(&current) {
        if METHODS.split('|').any(|c| c.eq_ignore_ascii_case(&current)) {
            metadata_dict.insert("FRAGMENTATIONMETHODE".to_string(), canonical_method(&current));
            return metadata_dict;
        }
        if !method_tokens(&current, &METHOD_TOKEN_ANY_CASE).is_empty() || METHOD_WORDS.is_match(&current) {
            // Méthode écrite autrement (« LOW-ENERGY CID ») : gardée telle quelle
            return metadata_dict;
        }
        // Pas une méthode : vidée
        metadata_dict.insert("FRAGMENTATIONMETHODE".to_string(), String::new());
    }
    let mut parts: Vec<String> = ["INSTRUMENT", "INSTRUMENTTYPE", crate::globals_vars::INSTRUMENT_HINTS_KEY, crate::globals_vars::IN_SILICO_HINTS_KEY]
        .iter().map(|k| get(&metadata_dict, k)).filter(|v| !v.is_empty()).collect();
    if let Some(c) = COMMENT_ORIGIN.captures(&get(&metadata_dict, "COMMENT")) {
        let origin = c[1].trim().to_string();
        if !origin.is_empty() {
            parts.push(origin);
        }
    }
    let text = parts.join(" | ");
    let method = if IN_SOURCE.is_match(&text) {
        Some("in-source CID".to_string())
    } else {
        match method_tokens(&text, &METHOD_TOKEN).as_slice() {
            [m] => Some(m.clone()),
            _ => {
                // Aucune méthode en majuscules, ou plusieurs : seulement celle qui commence un nom de bibliothèque
                // ou de fichier, en minuscules aussi (« uvpd.msp », « hcd_lib.msp »)
                let mut lib: Vec<String> = Vec::new();
                for piece in text.split(|c| c == '|' || c == ';' || c == ',') {
                    if let Some(c) = LIBRARY_PIECE.captures(piece.trim()) {
                        let m = canonical_method(&c[1]);
                        if !lib.contains(&m) {
                            lib.push(m);
                        }
                    }
                }
                if lib.len() == 1 { lib.pop() } else { None }
            }
        }
    };
    if let Some(m) = method {
        metadata_dict.insert("FRAGMENTATIONMETHODE".to_string(), m);
        return metadata_dict;
    }
    let name = get(&metadata_dict, "NAME");
    let found: Vec<(String, String)> = NAME_METHOD_ENERGY.captures_iter(&name).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    if found.len() == 1 && method_tokens(&name, &METHOD_TOKEN).len() == 1 {
        let (m, e) = &found[0];
        metadata_dict.insert("FRAGMENTATIONMETHODE".to_string(), m.clone());
        if empty(&get(&metadata_dict, "COLLISIONENERGY")) {
            metadata_dict.insert("COLLISIONENERGY".to_string(), e.clone());
        }
    }
    metadata_dict
}

/// Analyseur écrit après l'énergie (« 35 eV FT-MS ») : retiré de COLLISIONENERGY, mis dans INSTRUMENTTYPE
/// seulement si ce champ est vide (jamais écrasé). À appeler avant la normalisation des instruments.
pub fn analyzer_from_collision_energy(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let Some(value) = metadata_dict.get("COLLISIONENERGY").cloned() else { return metadata_dict };
    let Some(c) = ENERGY_ANALYZER.captures(value.trim()) else { return metadata_dict };
    let (energy, analyzer) = (c[1].trim().to_string(), c[2].to_string());
    metadata_dict.insert("COLLISIONENERGY".to_string(), energy);
    let current = metadata_dict.get("INSTRUMENTTYPE").map(|s| s.trim().to_string()).unwrap_or_default();
    if crate::globals_vars::EMPTY_PATTERN.is_match(&current) {
        metadata_dict.insert("INSTRUMENTTYPE".to_string(), analyzer);
    }
    metadata_dict
}

/// Uniformise COLLISIONENERGY (voir en tête de fichier) ; la méthode écrite avec l'énergie va dans
/// FRAGMENTATIONMETHODE si ce champ est vide.
pub fn normalize_collision_energy(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let Some(value) = metadata_dict.get("COLLISIONENERGY").cloned() else { return metadata_dict };
    if crate::globals_vars::EMPTY_PATTERN.is_match(&value) {
        return metadata_dict;
    }
    if let Some((energy, method)) = tidy(&value) {
        metadata_dict.insert("COLLISIONENERGY".to_string(), energy);
        if let Some(m) = method {
            let current = metadata_dict.get("FRAGMENTATIONMETHODE").map(|s| s.trim().to_string()).unwrap_or_default();
            if crate::globals_vars::EMPTY_PATTERN.is_match(&current) {
                metadata_dict.insert("FRAGMENTATIONMETHODE".to_string(), m);
            }
        }
    }
    metadata_dict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ce(v: &str) -> String {
        let mut m = HashMap::new();
        m.insert("COLLISIONENERGY".to_string(), v.to_string());
        normalize_collision_energy(m)["COLLISIONENERGY"].clone()
    }

    fn ce_method(v: &str, method: &str) -> (String, String) {
        let mut m = HashMap::new();
        m.insert("COLLISIONENERGY".to_string(), v.to_string());
        m.insert("FRAGMENTATIONMETHODE".to_string(), method.to_string());
        let m = normalize_collision_energy(m);
        (m["COLLISIONENERGY"].clone(), m["FRAGMENTATIONMETHODE"].clone())
    }

    #[test]
    fn valeurs_simples() {
        // Écritures réelles relevées par l'audit (MassBank, MoNA, MSnLib, GNPS...)
        assert_eq!(ce("40"), "40");
        assert_eq!(ce("35.0"), "35");
        assert_eq!(ce("35 eV"), "35 eV");
        assert_eq!(ce("20.0eV"), "20 eV");
        assert_eq!(ce("20.0V"), "20 V");
        assert_eq!(ce("20 V"), "20 V");
        assert_eq!(ce("-1.0"), "-1");
        assert_eq!(ce("17.60 eV"), "17.6 eV");
    }

    #[test]
    fn pourcentages_nce_et_nominal_ecrits_nce() {
        assert_eq!(ce("75 % (nominal)"), "75 NCE");
        assert_eq!(ce("130% (nominal)"), "130 NCE");
        assert_eq!(ce("30%"), "30 NCE");
        assert_eq!(ce("35  (nominal)"), "35 NCE");
        assert_eq!(ce("10(NCE)"), "10 NCE");
        assert_eq!(ce("30.0 NCE"), "30 NCE");
        assert_eq!(ce("NCE=80%"), "80 NCE");
    }

    #[test]
    fn rampes() {
        assert_eq!(ce("Ramp 5-60 V"), "Ramp 5/60 V");
        assert_eq!(ce("RAMP 20-30 eV"), "Ramp 20/30 eV");
        assert_eq!(ce("Ramp 17.6-26.4 eV"), "Ramp 17.6/26.4 eV");
        assert_eq!(ce("Ramp 20%-70% (nominal)"), "Ramp 20/70 NCE");
        assert_eq!(ce("10->20V"), "Ramp 10/20 V");
        assert_eq!(ce("30->35%"), "Ramp 30/35 NCE");
        assert_eq!(ce("Ramped collision energies of 20 to 40 V"), "Ramp 20/40 V");
        assert_eq!(ce("10-35,ramp"), "Ramp 10/35");
        // Deux valeurs reliées par un tiret : rampe
        assert_eq!(ce("20-40eV"), "Ramp 20/40 eV");
        assert_eq!(ce("30-50"), "Ramp 30/50");
        assert_eq!(ce("20-40%"), "Ramp 20/40 NCE");
    }

    #[test]
    fn paliers_et_listes_neutres() {
        // « stepped » écrit, ou trois valeurs et plus reliées par des tirets (Thermo)
        assert_eq!(ce("stepped 10, 30, 50"), "Stepped 10/30/50");
        assert_eq!(ce_method("HCD (NCE 20-30-40%)", ""), ("Stepped 20/30/40 NCE".into(), "HCD".into()));
        // Listes sans indication : neutres
        assert_eq!(ce("20.0, 30.0, 60.0"), "20/30/60");
        assert_eq!(ce("[20.0, 30.0, 60.0, 40.0]"), "20/30/60/40");
        assert_eq!(ce("20, 30, 45"), "20/30/45");
        assert_eq!(ce("15 / 50"), "15/50");
        assert_eq!(ce("0.60/0.55"), "0.6/0.55");
        assert_eq!(ce("15,30,45 (NCE)"), "15/30/45 NCE");
        assert_eq!(ce("-10, -35, -60V"), "-10/-35/-60 V");
        assert_eq!(ce("-60,-40,-20"), "-60/-40/-20");
        assert_eq!(ce("10 20"), "10/20");
        // Spectraverse, après lecture (rampe : champs 1 et 2 ; paliers : champs 1, 2, 3)
        assert_eq!(ce("Ramp 20.0, 40.0"), "Ramp 20/40");
        assert_eq!(ce("Stepped 5.361791, 8.042687, 10.723583"), "Stepped 5.361791/8.042687/10.723583");
        // GNPS : énergies collées
        assert_eq!(ce("102040"), "10/20/40");
    }

    #[test]
    fn methode_ecrite_avec_l_energie() {
        assert_eq!(ce_method("35 CID", ""), ("35".into(), "CID".into()));
        assert_eq!(ce_method("65HCD", ""), ("65".into(), "HCD".into()));
        assert_eq!(ce_method("hcd30.00", ""), ("30".into(), "HCD".into()));
        assert_eq!(ce_method("120 HCD", ""), ("120".into(), "HCD".into()));
        // Méthode déjà connue : gardée
        assert_eq!(ce_method("35 CID", "HCD"), ("35".into(), "HCD".into()));
    }

    #[test]
    fn ce_colle_et_tirets_bas() {
        assert_eq!(ce("CE75"), "75");
        assert_eq!(ce("NCE_20_30_40"), "20/30/40 NCE");
        assert_eq!(ce("CE0"), "CE0");
    }

    fn fm(pairs: &[(&str, &str)]) -> (String, String) {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let m = fragmentation_method_from_text(m);
        (m.get("FRAGMENTATIONMETHODE").cloned().unwrap_or_default(), m.get("COLLISIONENERGY").cloned().unwrap_or_default())
    }

    #[test]
    fn methode_lue_ailleurs_que_dans_son_champ() {
        // GNPS : instrument / type d'instrument (valeurs réelles)
        assert_eq!(fm(&[("INSTRUMENT", "HCD; Lumos")]).0, "HCD");
        assert_eq!(fm(&[("INSTRUMENTTYPE", "LC-ESI-CID; Velos")]).0, "CID");
        assert_eq!(fm(&[("INSTRUMENTTYPE", "ESI-HCD")]).0, "HCD");
        assert_eq!(fm(&[("INSTRUMENTTYPE", "in source CID")]).0, "in-source CID");
        // MS-DIAL / MoNA : origine, bibliothèque (clé interne des indices in silico)
        assert_eq!(fm(&[("IN_SILICO_HINTS", "HCD natural product library, Bruker")]).0, "HCD");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "UVPD on Thermo IQX | UVPD Library | uvpd.msp")]).0, "UVPD");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "TUR-UMN-024_dG-ONE_HCD_60.txt")]).0, "HCD");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "EAD PFP NP lib")]).0, "EAD");
        // Nom GNPS : méthode et énergie collées
        assert_eq!(fm(&[("NAME", "Catechin_HCD40_170906")]), ("HCD".into(), "40".into()));
        assert_eq!(fm(&[("NAME", "Catechin_HCD40_170906"), ("COLLISIONENERGY", "35")]), ("HCD".into(), "35".into()));
        // Méthode déjà écrite : gardée, mise en forme
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "cid"), ("INSTRUMENT", "HCD; Lumos")]).0, "CID");
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "ethcd")]).0, "EThcD");
        // Méthode écrite autrement : gardée telle quelle
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "LOW-ENERGY CID")]).0, "LOW-ENERGY CID");
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "collision-induced dissociation")]).0, "collision-induced dissociation");
        // Pas une méthode (analyseur, MoNA NIST-Glycan) : vidée
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "IT/ion trap")]).0, "");
        assert_eq!(fm(&[("FRAGMENTATIONMETHODE", "IT-FT/ion trap with FTMS")]).0, "");
        // Origine dans le commentaire (MS-DIAL, MoNA MSP) ; plusieurs méthodes : celle du nom de bibliothèque
        let bruker = "DB#=Bruker_HCD_library001003; origin=HCD natural product library, acquired in adjunct with UVPD library.";
        assert_eq!(fm(&[("COMMENT", bruker)]).0, "HCD");
        assert_eq!(fm(&[("COMMENT", "\"SMILES=C\" \"origin=uvpd.msp\" \"MoNA Rating=5.0\"")]).0, "UVPD");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "HCD natural product library, acquired in adjunct with UVPD library | hcd_lib.msp | HCD_natural_product_library")]).0, "HCD");
    }

    fn an(pairs: &[(&str, &str)]) -> (String, String) {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let m = normalize_collision_energy(analyzer_from_collision_energy(m));
        (m.get("COLLISIONENERGY").cloned().unwrap_or_default(), m.get("INSTRUMENTTYPE").cloned().unwrap_or_default())
    }

    #[test]
    fn analyseur_retire_de_l_energie() {
        // Type d'instrument déjà écrit : jamais écrasé
        assert_eq!(an(&[("COLLISIONENERGY", "35 eV FT-MS"), ("INSTRUMENTTYPE", "LC-ESI-ITFT")]), ("35 eV".into(), "LC-ESI-ITFT".into()));
        assert_eq!(an(&[("COLLISIONENERGY", "35 eV FT-MS II"), ("INSTRUMENTTYPE", "LC-ESI-ITFT")]), ("35 eV".into(), "LC-ESI-ITFT".into()));
        // Type d'instrument vide : l'analyseur y va
        assert_eq!(an(&[("COLLISIONENERGY", "35 eV IT-MS")]), ("35 eV".into(), "IT-MS".into()));
        // Sans analyseur : rien ne change
        assert_eq!(an(&[("COLLISIONENERGY", "3 kV")]), ("3 kV".into(), "".into()));
    }

    #[test]
    fn methode_pas_prise_sans_contexte_sur() {
        // Identifiants PubChem
        assert_eq!(fm(&[("IN_SILICO_HINTS", "PUBCHEM:CID: 3960")]).0, "");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "pubchem cid=71485")]).0, "");
        assert_eq!(fm(&[("INSTRUMENT", "CID 638")]).0, "");
        // Mots qui contiennent un sigle, minuscules isolées, plusieurs méthodes
        assert_eq!(fm(&[("IN_SILICO_HINTS", "acid library, decade")]).0, "");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "Cid lab")]).0, "");
        assert_eq!(fm(&[("INSTRUMENT", "CID; HCD")]).0, "");
        assert_eq!(fm(&[("NAME", "Quercetin_465_[C20H18O13-H]-_CID25_MS3-465_HCD100_46")]), ("".into(), "".into()));
        // Nom qui commence par « CID90 » (ambigu, laissé) ; nom sans énergie collée
        assert_eq!(fm(&[("NAME", "CID90; (Acyl-CoA); [M+H]+")]).0, "");
        assert_eq!(fm(&[("NAME", "HCD natural compound")]).0, "");
        // Plusieurs méthodes sans nom de bibliothèque qui en commence une : rien
        assert_eq!(fm(&[("COMMENT", "origin=mixed CID and HCD runs")]).0, "");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "Cid lab | HCD x | CID y")]).0, "");
        assert_eq!(fm(&[("IN_SILICO_HINTS", "cid lab | hcd notes")]).0, "");
    }

    #[test]
    fn ecritures_laissees_telles_quelles() {
        for v in ["CE0", "low", "high", "3 kV", "Ramp 10-30 kV", "10-30V+20V", "scaled by m/z",
                  "15, 30, 45, 60, 70 or 90 (nominal)", "scaled 80 (m/z=200) to 110 (m/z=120)", "1000", "2005"] {
            assert_eq!(ce(v), v, "{v}");
        }
        // Vide : rien
        assert_eq!(ce(""), "");
    }
}
