// src/normalizer/normalize_name.rs
use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;

// Énergie de collision écrite en texte : un nombre, ou une plage / liste de nombres ("10-30", "20/40", "15,30"),
// suivi de l'unité (eV, V, NCE) ; plusieurs reliées par « + » ("10-30V+20V").
const NUM: &str = r"\d+(?:[.,]\d+)?";
fn range() -> String { format!(r"{NUM}(?:\s*(?:-|–|to|/|,)\s*{NUM})*") }
fn with_unit() -> String {
    let one = format!(r"{}\s*(?:eV|V|NCE)\b", range());
    format!(r"{one}(?:\s*\+\s*{one})*")
}

/// Énergie en fin de nom (GNPS, Metaboanalyst) : "TYRAMINE - 20.0 eV", "LIPOAMIDE - 50.0 eV M+H",
/// "Hydroxybenzaldehyde 20eV M-H", "Asperterrol(10-30V+20V)", "Preterramide C(10-30V+35V) M+Na", "… 40 NCE".
/// L'unité est obligatoire ; un adduit peut suivre (il reste dans le nom pour la lecture de l'adduit).
static NAME_TRAILING_ENERGY: Lazy<Regex> = Lazy::new(|| Regex::new(&format!(
    r"(?i)^(?P<head>.*?\S)(?:\s*-\s+|\s+|\s*\()(?P<e>{})\)?(?P<tail>\s+\[?\d*M[+-][^\s]*)?\s*$", with_unit())).unwrap());

/// Morceau de nom MS-DIAL (séparé par « ; ») qui n'est qu'une énergie : "CE30", "CE 20V", "CE12.5", "30eV".
static ENERGY_PIECE: Lazy<Regex> = Lazy::new(|| Regex::new(&format!(
    r"(?i)^(?:CE\s*[:=]?\s*(?P<a>{}(?:\s*(?:eV|V|NCE)\b)?)|(?P<b>{}))$", range(), with_unit())).unwrap());

/// Énergie dans un commentaire (MassBank surtout) : seulement des écritures sans ambiguïté.
static COMMENT_ENERGY: Lazy<Vec<Regex>> = Lazy::new(|| {
    let value = format!(r"(?:ramp\s*)?{}(?:\s*(?:eV|V|NCE)\b)?", range());
    vec![
        // MassBank : "PROCESSING averaging of repeated ion fragments at 30.0 NCE within 5 ppm window" (ou "merged")
        Regex::new(&format!(r"(?i)\brepeated ion fragments at\s+(?P<e>{NUM}\s*(?:NCE|eV|V))\b")).unwrap(),
        // "CollisionEnergy:102040", "collision energy: 40 eV", "collisionenergy=90" (pas "collision energy 2=81")
        Regex::new(&format!(r"(?i)\bcollision[\s_-]?energy\s*[:=]\s*(?P<e>{value})")).unwrap(),
        // "CE:10 V", "CE:Ramp 5-60 V"
        Regex::new(&format!(r"(?i)(?:^|[^\w])CE\s*[:=]\s*(?P<e>{value})")).unwrap(),
    ]
});

/// Mention des bibliothèques GNPS de synthèse multiplexée en fin de nom :
/// "Acetyl-L-Carnitine_palmitic-acid (known isomers: 0; isobaric peaks: 1)".
static NAME_ISOMERS_NOTE: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?i)^(?P<head>.*?\S)\s*\((?P<note>(?:known isomers|isobaric peaks)[^()]*)\)\s*$").unwrap());

fn is_empty(v: &str) -> bool {
    crate::globals_vars::EMPTY_PATTERN.is_match(v)
}

/// Spectre GC / EI : une énergie en eV écrite dans le nom ou le commentaire y est l'énergie d'ionisation
/// (70 eV), pas une énergie de collision : rien n'est lu.
fn is_gc(metadata_dict: &HashMap<String, String>) -> bool {
    let get = |k: &str| metadata_dict.get(k).map(|s| s.as_str()).unwrap_or("");
    crate::globals_vars::is_gc_spectrum(get("FILENAME"), get("INSTRUMENTTYPE"))
}

/// Retire du nom ce qui n'est pas le nom de la molécule, sans perdre l'information.
///
/// - GNPS écrit parfois l'énergie de collision dans le nom : "Cholic acid CollisionEnergy:102040". Elle est
///   retirée du nom et mise dans COLLISIONENERGY si ce champ est vide (telle qu'elle est écrite : sa mise en
///   forme sera traitée à part). Avant, seul le JSON GNPS le faisait ; en MSP et MGF, elle restait dans le nom.
/// - MS-DIAL (MSMS-Public) écrit parfois l'énergie de collision et l'InChIKey en fin de nom, séparées par « ; » :
///   "Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N" -> "Piperine", "Betaine; CE30; CE10; KWIU…-N" -> "Betaine"
///   (énergies "30, 10" mises dans COLLISIONENERGY s'il est vide ; "CE0" est repris tel quel, "CE0").
///   Seuls les derniers morceaux qui sont exactement une InChIKey ou une énergie ("CE30", "CE 20V", "30eV") sont
///   retirés : deux noms séparés par « ; » ("Isosaxalin; Heraclenol") restent tels quels, et le premier morceau
///   est toujours gardé. Une énergie au milieu ("Sucrose; AIF; 30eV; MS2Dec") est lue, le nom reste tel quel.
/// - Bibliothèques GNPS de synthèse multiplexée : "A_B (known isomers: 0; isobaric peaks: 1)" -> "A_B", la mention
///   étant ajoutée à la fin de COMMENT. Elle décrit l'annotation de ce spectre (structure choisie parmi N isomères),
///   pas la molécule : elle ne doit pas passer dans le nom (ni dans la base interne, qui reprend les noms).
/// - GNPS / Metaboanalyst : énergie avec son unité en fin de nom ("TYRAMINE - 20.0 eV",
///   "LIPOAMIDE - 50.0 eV M+H" -> "LIPOAMIDE M+H", "Asperterrol(10-30V+20V)" -> "Asperterrol"). Pas pour un
///   spectre GC / EI (70 eV = énergie d'ionisation).
pub fn clean_name_suffixes(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let Some(name) = metadata_dict.get("NAME").cloned() else { return metadata_dict };
    let gc = is_gc(&metadata_dict);
    let mut energies: Vec<String> = Vec::new();
    let mut name_out = name.clone();
    if let Some(c) = NAME_ISOMERS_NOTE.captures(&name) {
        let note = c["note"].trim().to_string();
        name_out = c["head"].trim_end().to_string();
        let comment = metadata_dict.get("COMMENT").map(|s| s.trim().to_string()).unwrap_or_default();
        let comment = if is_empty(&comment) { note } else { format!("{comment}; {note}") };
        metadata_dict.insert("COMMENT".to_string(), comment);
    }
    let probe = name_out.clone();
    if let Some(caps) = crate::globals_vars::GNPS_COLLISION_ENERGY_EXTRACT_PATTERN.captures(&probe) {
        if let Some(e) = caps.get(1) {
            let real = crate::globals_vars::GNPS_COLLISION_ENERGY_EXTRACT_PATTERN.replace(&probe, " ").trim().to_string();
            if !real.is_empty() {
                energies.push(e.as_str().trim().to_string());
                name_out = real;
            }
        }
    }
    let (mut cleaned, ce) = strip_suffixes(&name_out);
    energies.extend(ce);
    if energies.is_empty() && !gc {
        if let Some(c) = NAME_TRAILING_ENERGY.captures(&cleaned) {
            let head = c["head"].trim_end();
            // "…; CE 20V" est un morceau MS-DIAL (déjà traité), pas une énergie collée au nom
            if !head.to_uppercase().ends_with("CE") {
                energies.push(c["e"].trim().to_string());
                let tail = c.name("tail").map(|t| t.as_str().trim()).unwrap_or("");
                cleaned = if tail.is_empty() { head.to_string() } else { format!("{head} {tail}") };
            }
        }
    }
    if energies.is_empty() {
        energies.extend(middle_energies(&cleaned));
    }
    if cleaned != name {
        metadata_dict.insert("NAME".to_string(), cleaned);
    }
    let current = metadata_dict.get("COLLISIONENERGY").map(|s| s.trim().to_string()).unwrap_or_default();
    let energies: Vec<String> = energies.into_iter().filter(|e| !is_empty(e)).collect();
    if !energies.is_empty() && is_empty(&current) {
        metadata_dict.insert("COLLISIONENERGY".to_string(), energies.join(", "));
    }
    metadata_dict
}

/// Énergie de collision vide : lue dans le commentaire, seulement sous une écriture sans ambiguïté
/// ("averaging of repeated ion fragments at 30.0 NCE", "CollisionEnergy:102040", "CE:Ramp 5-60 V"), telle qu'elle est
/// écrite. Le commentaire n'est pas modifié. Pas pour un spectre GC / EI.
pub fn collision_energy_from_comment(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let current = metadata_dict.get("COLLISIONENERGY").map(|s| s.trim().to_string()).unwrap_or_default();
    if !is_empty(&current) || is_gc(&metadata_dict) {
        return metadata_dict;
    }
    let Some(comment) = metadata_dict.get("COMMENT").cloned() else { return metadata_dict };
    for rx in COMMENT_ENERGY.iter() {
        if let Some(e) = rx.captures(&comment).and_then(|c| c.name("e").map(|m| m.as_str().trim().to_string())) {
            if !is_empty(&e) {
                metadata_dict.insert("COLLISIONENERGY".to_string(), e);
                break;
            }
        }
    }
    metadata_dict
}

/// Énergie d'un morceau de nom ("CE30" -> "30", "CE 20V" -> "20V", "30eV" -> "30eV"), ou None.
/// "CE0" est gardé tel qu'il est écrit ("CE0") : « 0 » seul serait pris pour une valeur vide, alors que
/// MS-DIAL indique bien une énergie nulle (canal basse énergie de l'AIF).
fn piece_energy(s: &str) -> Option<String> {
    let s = s.trim();
    let c = ENERGY_PIECE.captures(s)?;
    let e = c.name("a").or_else(|| c.name("b")).map(|m| m.as_str().trim().to_string())?;
    Some(if c.name("a").is_some() && is_empty(&e) { s.to_string() } else { e })
}

/// Nom sans ses derniers morceaux InChIKey / énergie ; énergies trouvées, dans l'ordre du nom.
fn strip_suffixes(name: &str) -> (String, Vec<String>) {
    let mut end = name.trim_end();
    let mut energies = Vec::new();
    while let Some(cut) = end.rfind(';') {
        let last = end[cut + 1..].trim();
        let head = end[..cut].trim_end();
        let energy = piece_energy(last);
        if head.is_empty() || !(is_inchikey(last) || energy.is_some()) {
            break;
        }
        if let Some(e) = energy {
            energies.insert(0, e);
        }
        end = head;
    }
    let out = if end.len() == name.trim_end().len() { name.to_string() } else { end.to_string() };
    (out, energies)
}

/// Énergies écrites dans un morceau du milieu du nom ("Sucrose; AIF; 30eV; MS2Dec"), premier morceau exclu.
fn middle_energies(name: &str) -> Vec<String> {
    name.split(';').skip(1).filter_map(piece_energy).collect()
}

fn is_inchikey(s: &str) -> bool {
    crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(n: &str) -> String {
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), n.to_string());
        clean_name_suffixes(m)["NAME"].clone()
    }

    fn name_ce(n: &str, ce: &str) -> (String, String) {
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), n.to_string());
        m.insert("COLLISIONENERGY".to_string(), ce.to_string());
        let m = clean_name_suffixes(m);
        (m["NAME"].clone(), m["COLLISIONENERGY"].clone())
    }

    #[test]
    fn mention_known_isomers_passee_du_nom_au_commentaire() {
        // GNPS, bibliothèques de synthèse multiplexée (CCMSLIB00017390183, CCMSLIB00017390599)
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), "Acetyl-L-Carnitine_eicosanoic-acid (known isomers: 0; isobaric peaks: 1)".to_string());
        let m = clean_name_suffixes(m);
        assert_eq!(m["NAME"], "Acetyl-L-Carnitine_eicosanoic-acid");
        assert_eq!(m["COMMENT"], "known isomers: 0; isobaric peaks: 1");
        // Commentaire déjà rempli : mention ajoutée à la fin ; "isobaric peaks in run" aussi
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), "Serotonin_acetoacetamide (known isomers: 0; isobaric peaks in run: 2)".to_string());
        m.insert("COMMENT".to_string(), "autre".to_string());
        let m = clean_name_suffixes(m);
        assert_eq!(m["NAME"], "Serotonin_acetoacetamide");
        assert_eq!(m["COMMENT"], "autre; known isomers: 0; isobaric peaks in run: 2");
        // Parenthèse ordinaire en fin de nom : inchangée, commentaire non créé
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), "Quercetin (aglycone)".to_string());
        let m = clean_name_suffixes(m);
        assert_eq!(m["NAME"], "Quercetin (aglycone)");
        assert!(!m.contains_key("COMMENT"));
    }

    #[test]
    fn energie_du_nom_gardee_pour_tous_les_formats() {
        // GNPS (même règle qu'avant en JSON, maintenant aussi en MSP et MGF)
        assert_eq!(name_ce("Cholic acid CollisionEnergy:102040", ""), ("Cholic acid".into(), "102040".into()));
        assert_eq!(name_ce("Cholic acid CollisionEnergy:102040", "35"), ("Cholic acid".into(), "35".into()));
        // MS-DIAL : énergies en fin de nom
        assert_eq!(name_ce("Betaine; CE30; CE10; KWIUHFFTVRNATP-UHFFFAOYSA-N", ""), ("Betaine".into(), "30, 10".into()));
        // "CE0" repris tel quel ; un champ « vide » écrit (0) est remplacé par une vraie valeur
        assert_eq!(name_ce("Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N", ""), ("Piperine".into(), "CE0".into()));
        assert_eq!(name_ce("Truc; CE20", "0"), ("Truc".into(), "20".into()));
    }

    #[test]
    fn energie_et_inchikey_retirees_du_nom() {
        // Noms réels de MSMS-Public (MS-DIAL)
        assert_eq!(name("Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N"), "Piperine");
        assert_eq!(name("Betaine; CE30; CE10; KWIUHFFTVRNATP-UHFFFAOYSA-N"), "Betaine");
        assert_eq!(name("Truc; MXXWOMGUGJBKIW-YPCIICBESA-N"), "Truc");
        assert_eq!(name("Truc; CE12.5"), "Truc");
    }

    #[test]
    fn autres_noms_inchanges() {
        assert_eq!(name("Isosaxalin; Heraclenol"), "Isosaxalin; Heraclenol");
        assert_eq!(name("Chrysin"), "Chrysin");
        assert_eq!(name("PC 34:1; CE"), "PC 34:1; CE");
        assert_eq!(name("Acide; CEtone"), "Acide; CEtone");
        // Le nom n'est jamais vidé
        assert_eq!(name("MXXWOMGUGJBKIW-YPCIICBESA-N"), "MXXWOMGUGJBKIW-YPCIICBESA-N");
        assert_eq!(name("CE10; MXXWOMGUGJBKIW-YPCIICBESA-N"), "CE10");
    }

    fn name_ce_meta(n: &str, extra: &[(&str, &str)]) -> (String, String) {
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), n.to_string());
        for (k, v) in extra { m.insert(k.to_string(), v.to_string()); }
        let m = clean_name_suffixes(m);
        (m["NAME"].clone(), m.get("COLLISIONENERGY").cloned().unwrap_or_default())
    }

    #[test]
    fn energie_avec_unite_en_fin_de_nom() {
        // GNPS / Metaboanalyst (noms réels de l'échantillon)
        assert_eq!(name_ce("TYRAMINE - 20.0 eV", ""), ("TYRAMINE".into(), "20.0 eV".into()));
        assert_eq!(name_ce("LIPOAMIDE - 50.0 eV M+H", ""), ("LIPOAMIDE M+H".into(), "50.0 eV".into()));
        assert_eq!(name_ce("4-Hydroxybenzaldehyde 20eV M-H", ""), ("4-Hydroxybenzaldehyde M-H".into(), "20eV".into()));
        assert_eq!(name_ce("(-)-Citramalic acid 60eV", ""), ("(-)-Citramalic acid".into(), "60eV".into()));
        assert_eq!(name_ce("Asperterrol(10-30V+20V)", ""), ("Asperterrol".into(), "10-30V+20V".into()));
        assert_eq!(name_ce("Preterramide C(10-30V+35V) M+Na", ""), ("Preterramide C M+Na".into(), "10-30V+35V".into()));
        assert_eq!(name_ce("N-methyl-indole-3-carboxamide 40 NCE", ""), ("N-methyl-indole-3-carboxamide".into(), "40 NCE".into()));
        // Énergie déjà connue : gardée, le nom est quand même nettoyé
        assert_eq!(name_ce("TYRAMINE - 20.0 eV", "35"), ("TYRAMINE".into(), "35".into()));
        // Spectre GC / EI : rien n'est lu (énergie d'ionisation)
        assert_eq!(name_ce_meta("Toluene 70eV", &[("INSTRUMENTTYPE", "GC-EI-TOF")]), ("Toluene 70eV".into(), "".into()));
    }

    #[test]
    fn energie_ms_dial_avec_espace_ou_unite() {
        assert_eq!(name_ce("PC 16:0/16:0; [M+NH4]+; CE 20V", ""), ("PC 16:0/16:0; [M+NH4]+".into(), "20V".into()));
        assert_eq!(name_ce("Truc; 30eV", ""), ("Truc".into(), "30eV".into()));
        // Au milieu du nom : lue, le nom reste tel quel ; CE0 repris tel quel
        assert_eq!(name_ce("Sucrose; AIF; 30eV; MS2Dec", ""), ("Sucrose; AIF; 30eV; MS2Dec".into(), "30eV".into()));
        assert_eq!(name_ce("Cyanocobalamin; AIF; CE0; MS2Dec", ""), ("Cyanocobalamin; AIF; CE0; MS2Dec".into(), "CE0".into()));
    }

    #[test]
    fn noms_sans_energie_inchanges() {
        // Pas d'unité : pas une énergie
        assert_eq!(name_ce("Compound 20", ""), ("Compound 20".into(), "".into()));
        assert_eq!(name_ce("PC 34:1", ""), ("PC 34:1".into(), "".into()));
        assert_eq!(name_ce("Vitamin D3", ""), ("Vitamin D3".into(), "".into()));
        assert_eq!(name_ce("CE 14:1", ""), ("CE 14:1".into(), "".into()));
        // Le nom n'est jamais vidé
        assert_eq!(name_ce("20 eV", ""), ("20 eV".into(), "".into()));
    }

    fn from_comment(comment: &str, ce: &str, extra: &[(&str, &str)]) -> String {
        let mut m = HashMap::new();
        m.insert("COMMENT".to_string(), comment.to_string());
        m.insert("COLLISIONENERGY".to_string(), ce.to_string());
        for (k, v) in extra { m.insert(k.to_string(), v.to_string()); }
        collision_energy_from_comment(m)["COLLISIONENERGY"].clone()
    }

    #[test]
    fn energie_lue_dans_le_commentaire() {
        // MassBank RIKEN (commentaires réels)
        assert_eq!(from_comment("DB#=MSBNK-ISAS_Dortmund-IA000095; origin=MassBank; PROCESSING averaging of repeated ion fragments at 30.0 NCE within 5 ppm window [MS, MS:1000575, mean of spectra, ]", "", &[]), "30.0 NCE");
        assert_eq!(from_comment("merged repeated ion fragments at 30.0 eV within 5 ppm window", "", &[]), "30.0 eV");
        assert_eq!(from_comment("C11H12N2O2; tryptophan CollisionEnergy:102040; Ranking 1", "", &[]), "102040");
        assert_eq!(from_comment("LC-ESI-QTOF; MS2; CE:Ramp 5-60 V; [M-H]-", "", &[]), "Ramp 5-60 V");
        assert_eq!(from_comment("\"collisiontype=hcd\" \"collisionenergy=90\"", "", &[]), "90");
        // Énergie déjà connue, autre nombre, ou spectre GC : rien
        assert_eq!(from_comment("merged repeated ion fragments at 30.0 NCE", "20", &[]), "20");
        assert_eq!(from_comment("\"collision energy 2=81.0731\"", "", &[]), "");
        assert_eq!(from_comment("CE:10 V", "", &[("INSTRUMENTTYPE", "GC-EI-TOF")]), "");
    }

}
