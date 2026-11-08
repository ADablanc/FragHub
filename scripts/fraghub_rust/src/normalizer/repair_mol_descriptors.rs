// src/normalizer/repair_mol_descriptors.rs
use std::collections::HashMap;

/// Répare la valeur 'INCHI' en s'assurant qu'elle commence par "InChI=".
///
/// Pour un développeur Python : Cette fonction remplace un bloc de texte avec une expression régulière.
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Le dictionnaire des métadonnées du spectre.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire avec le champ INCHI réparé.
pub fn repair_inchi(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    // `if let Some(inchi)` gère proprement le cas où la clé "INCHI" n'existerait pas dans le dictionnaire.
    // `.get()` retourne un `Option<&String>`, on extrait donc une référence vers le texte.
    if let Some(inchi) = metadata_dict.get("INCHI") {
        if !inchi.is_empty() {
            // Remplacement via regex équivalent au `re.sub` de Python.
            // `.into_owned()` convertit le type `Cow<str>` (Copy-On-Write) retourné par `replace` 
            // en une vraie `String` indépendante. C'est très optimisé en Rust.
            let repaired = crate::globals_vars::REPAIR_INCHI_PATTERN
                .replace(inchi, "InChI=")
                .into_owned();
                
            // `.insert()` remplace l'ancienne valeur si la clé existe déjà.
            // En Rust, "INCHI" est une vue statique (`&str`), on doit appeler `.to_string()` pour l'allouer sur le tas (`String`).
            metadata_dict.insert("INCHI".to_string(), repaired);
        }
    }
    metadata_dict
}

/// Corrige les erreurs de placement entre SMILES, INCHI et INCHIKEY.
/// 
/// Pour un développeur Python : Remplace les logiques de déplacement de champs. Au lieu de copier 
/// `dict["A"] = dict["B"]`, on clone (`.clone()`) car Rust interdit à deux variables d'être propriétaires
/// de la même chaîne de caractères en mémoire.
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Le dictionnaire des métadonnées du spectre.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire avec les descripteurs moléculaires aux bons endroits.
/// Retire les espaces et les guillemets qui entourent une valeur ("\"InChI=1S/...\"" -> "InChI=1S/...").
/// Cas réel (GNPS JSON) : un InChI écrit entre guillemets n'était plus reconnu comme InChI ; pris pour un
/// SMILES, il remplaçait le vrai SMILES du spectre et l'InChI était vidé.
fn strip_quotes(v: &str) -> &str {
    let mut s = v.trim();
    while s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''))) {
        s = s[1..s.len() - 1].trim();
    }
    s
}

/// Le texte a-t-il vraiment la forme d'un SMILES ? Hors crochets, seules les lettres des atomes du
/// sous-ensemble organique sont permises (B, C, N, O, P, S, F, Cl, Br, I et b, c, n, o, p, s) ; entre
/// crochets, tout atome ([Na+], [C@@H], [nH]...). Chiffres et symboles de liaison, cycles, charges et
/// stéréo permis ; pas d'espace. "ZZORFUFYDOWNEF-UHFFFAOYAR" (Z, R, U, Y...) ou un nom ne passent pas.
fn looks_like_smiles(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() { return false; }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut atoms = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' => {
                let close = match chars[i..].iter().position(|&x| x == ']') { Some(p) => i + p, None => return false };
                if close == i + 1 { return false; }
                atoms += 1;
                i = close + 1;
                continue;
            }
            'C' if chars.get(i + 1) == Some(&'l') => { atoms += 1; i += 2; continue; }
            'B' if chars.get(i + 1) == Some(&'r') => { atoms += 1; i += 2; continue; }
            'B' | 'C' | 'N' | 'O' | 'P' | 'S' | 'F' | 'I' | 'b' | 'c' | 'n' | 'o' | 'p' | 's' => atoms += 1,
            '0'..='9' | '(' | ')' | '=' | '#' | '$' | '/' | '\\' | '@' | '+' | '-' | '.' | '%' | ':' | '*' | '~' => {}
            _ => return false,
        }
        i += 1;
    }
    atoms > 0
}

pub fn repair_mol_descriptors(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    for key in ["SMILES", "INCHI", "INCHIKEY"] {
        if let Some(v) = metadata_dict.get(key) {
            let s = strip_quotes(v);
            if s.len() != v.len() {
                let s = s.to_string();
                metadata_dict.insert(key.to_string(), s);
            }
        }
    }
    // `.cloned()` crée une vraie copie de la String pour nous permettre de la manipuler.
    // `.unwrap_or_default()` renvoie une chaîne vide `""` si la clé n'existait pas (au lieu de crasher).
    let smiles = metadata_dict.get("SMILES").cloned().unwrap_or_default();
    let inchi = metadata_dict.get("INCHI").cloned().unwrap_or_default();
    let inchikey = metadata_dict.get("INCHIKEY").cloned().unwrap_or_default();

    // --- Vérification initiale ---
    if crate::globals_vars::SMILES_PATTERN.is_match(&smiles) &&
        crate::globals_vars::INCHI_PATTERN.is_match(&inchi) &&
        crate::globals_vars::INCHIKEY_PATTERN.is_match(&inchikey)
    {
        return repair_inchi(metadata_dict); // Sortie prématurée si tout est bon.
    }

    // --- Logique de réparation croisée (Cross-Field Repair) ---
    // Une valeur rangée dans le mauvais champ n'est déplacée que si elle a vraiment la forme attendue
    // (looks_like_smiles, INCHI_STRICT_PATTERN, INCHIKEY_FULL_PATTERN) ET si le bon champ est vide (ou, pour
    // l'InChIKey, ne contient pas d'InChIKey valide) : elle n'écrase jamais une valeur du fichier. Elle est
    // toujours retirée du mauvais champ ; RDKit recalcule ensuite ce qui manque à partir de la structure.
    // Cas réels : MoNA EMBL-MCF "inchikey = ZZORFUFYDOWNEF-UHFFFAOYAR" (InChIKey abîmée) prise pour un SMILES
    // et écrasant le bon SMILES ; GNPS JSON INCHI_AUX = "InChI=... [M-H]-" écrasant le bon InChI.
    let empty = |d: &HashMap<String, String>, k: &str| d.get(k).map_or(true, |v| v.trim().is_empty());

    // 1. SMILES trouvé dans le champ INCHI
    if looks_like_smiles(&inchi) && !crate::globals_vars::INCHI_PATTERN.is_match(&inchi)
        && !crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(&inchi)
    {
        if empty(&metadata_dict, "SMILES") {
            metadata_dict.insert("SMILES".to_string(), inchi.clone());
        }
        metadata_dict.insert("INCHI".to_string(), String::new());
    }

    // 2. SMILES trouvé dans le champ INCHIKEY
    if looks_like_smiles(&inchikey) && !crate::globals_vars::INCHI_PATTERN.is_match(&inchikey)
        && !crate::globals_vars::INCHIKEY_PATTERN.is_match(&inchikey)
    {
        if empty(&metadata_dict, "SMILES") {
            metadata_dict.insert("SMILES".to_string(), inchikey.clone());
        }
        metadata_dict.insert("INCHIKEY".to_string(), String::new());
    }

    // 3. INCHI trouvé dans le champ SMILES
    if crate::globals_vars::INCHI_PATTERN.is_match(&smiles) {
        if crate::globals_vars::INCHI_STRICT_PATTERN.is_match(&smiles) && empty(&metadata_dict, "INCHI") {
            metadata_dict.insert("INCHI".to_string(), smiles.clone());
        }
        metadata_dict.insert("SMILES".to_string(), String::new());
    }

    // 4. INCHI trouvé dans le champ INCHIKEY (GNPS JSON : INCHI_AUX = "InChI=... [M-H]-", parfois d'une autre
    //    molécule : pas un InChI valide à cause de l'adduit, il est seulement retiré)
    if crate::globals_vars::INCHI_PATTERN.is_match(&inchikey) {
        if crate::globals_vars::INCHI_STRICT_PATTERN.is_match(&inchikey) && empty(&metadata_dict, "INCHI") {
            metadata_dict.insert("INCHI".to_string(), inchikey.clone());
        }
        metadata_dict.insert("INCHIKEY".to_string(), String::new());
    }

    let has_key = |d: &HashMap<String, String>| d.get("INCHIKEY")
        .map_or(false, |v| crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(v));

    // 5. INCHIKEY trouvé dans le champ INCHI
    if crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(&inchi) {
        if !has_key(&metadata_dict) {
            metadata_dict.insert("INCHIKEY".to_string(), inchi.trim().to_string());
        }
        metadata_dict.insert("INCHI".to_string(), String::new());
    }

    // 6. INCHIKEY trouvé dans le champ SMILES
    if crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(&smiles) {
        if !has_key(&metadata_dict) {
            metadata_dict.insert("INCHIKEY".to_string(), smiles.trim().to_string());
        }
        metadata_dict.insert("SMILES".to_string(), String::new());
    }

    // Étape finale : on sécurise le préfixe de l'InChI
    repair_inchi(metadata_dict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(smiles: &str, inchi: &str, inchikey: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("SMILES".to_string(), smiles.to_string());
        m.insert("INCHI".to_string(), inchi.to_string());
        m.insert("INCHIKEY".to_string(), inchikey.to_string());
        m
    }

    #[test]
    fn inchi_entre_guillemets_reconnu() {
        // GNPS JSON (CCMSLIB00010152168) : INCHI écrit entre guillemets
        let smiles = "C[C@H]1CN(Cc2ccc(CC(=O)N3CCC(Nc4cc(F)ccc4)CC3)cc2)CCN1";
        let out = repair_mol_descriptors(meta(smiles, "\"InChI=1S/C25H33FN4O/c1-19-17-29\"", "RZKDEGZIFSJVNA-IBGZPJMESA-N"));
        assert_eq!(out["SMILES"], smiles);
        assert_eq!(out["INCHI"], "InChI=1S/C25H33FN4O/c1-19-17-29");
        assert_eq!(out["INCHIKEY"], "RZKDEGZIFSJVNA-IBGZPJMESA-N");
        // Valeur sans guillemets : inchangée
        let out = repair_mol_descriptors(meta(smiles, "InChI=1S/C25H33FN4O/c1", ""));
        assert_eq!(out["INCHI"], "InChI=1S/C25H33FN4O/c1");
    }

    #[test]
    fn lipid_smiles_is_not_taken_for_an_inchikey() {
        // Acide palmitique : 16 lettres consécutives, InChIKey absent.
        let out = repair_mol_descriptors(meta("CCCCCCCCCCCCCCCC(=O)O", "", ""));
        assert_eq!(out["SMILES"], "CCCCCCCCCCCCCCCC(=O)O");
        assert_eq!(out["INCHIKEY"], "");
    }

    #[test]
    fn pure_carbon_chain_smiles_is_kept() {
        // Tétradécane : exactement 14 "C", forme d'un InChIKey court.
        let out = repair_mol_descriptors(meta("CCCCCCCCCCCCCC", "", ""));
        assert_eq!(out["SMILES"], "CCCCCCCCCCCCCC");
        assert_eq!(out["INCHIKEY"], "");
    }

    #[test]
    fn stereo_smiles_is_not_taken_for_an_inchi() {
        // Acide oléique (double liaison cis notée avec "/C=C\\").
        let smiles = "CCCCCCCC/C=C\\CCCCCCCC(=O)O";
        let out = repair_mol_descriptors(meta(smiles, "", ""));
        assert_eq!(out["SMILES"], smiles);
        assert_eq!(out["INCHI"], "");
        // Autre forme stéréo avec "/C/".
        let smiles2 = "C/C=C/C(=O)O";
        let out2 = repair_mol_descriptors(meta(smiles2, "", ""));
        assert_eq!(out2["SMILES"], smiles2);
        assert_eq!(out2["INCHI"], "");
    }

    #[test]
    fn inchi_du_champ_inchikey_n_ecrase_pas_le_bon_inchi() {
        // Cas réel GNPS JSON (CCMSLIB00016154064) : INCHI_AUX, rangé dans INCHIKEY, est l'InChI d'une autre molécule
        let good = "InChI=1S/C11H14N2O7/c1-5(14)19-4-6-8(16)9(17)10(20-6)13-3-2-7(15)12-11(13)18/h2-3,6,8-10,16-17H,4H2,1H3,(H,12,15,18)";
        let aux = "InChI=1S/C9H11FN6O4/c10-5-6(18)9(3-17,14-15-12)20-7(5)16-2-1-4(11)13-8(16)19/h1-2,5-7,17-18H,3H2,(H2,11,13,19) [M-H]-";
        let out = repair_mol_descriptors(meta("CC(=O)OCC1OC(n2ccc(=O)[nH]c2=O)C(O)C1O", good, aux));
        assert_eq!(out["INCHI"], good);
        assert_eq!(out["INCHIKEY"], "");
        // INCHI vide : l'InChI du champ INCHIKEY est toujours récupéré
        let out = repair_mol_descriptors(meta("", "", good));
        assert_eq!(out["INCHI"], good);
        assert_eq!(out["INCHIKEY"], "");
    }

    #[test]
    fn inchikey_abimee_pas_prise_pour_un_smiles() {
        // Cas réel MoNA JSON (EMBL_MCF_2_0_HRMS_Library000498) : InChIKey abîmée du déposant
        let smiles = "COc1cc(nc(n1)OC)NS(=O)(=O)c2ccc(cc2)N";
        let out = repair_mol_descriptors(meta(smiles, "", "ZZORFUFYDOWNEF-UHFFFAOYAR"));
        assert_eq!(out["SMILES"], smiles);
        // Une vraie valeur mal rangée est déplacée seulement si le bon champ est vide
        let out = repair_mol_descriptors(meta("", "", smiles));
        assert_eq!(out["SMILES"], smiles);
        assert_eq!(out["INCHIKEY"], "");
        let out = repair_mol_descriptors(meta("CCO", "", "OCC"));
        assert_eq!(out["SMILES"], "CCO");
        assert_eq!(out["INCHIKEY"], "");
        // InChIKey dans INCHI : ne remplace pas une InChIKey valide
        let out = repair_mol_descriptors(meta("CCO", "AAAAAAAAAAAAAA-BBBBBBBBBB-N", "LFQSCWFLJHTTHZ-UHFFFAOYSA-N"));
        assert_eq!(out["INCHIKEY"], "LFQSCWFLJHTTHZ-UHFFFAOYSA-N");
        assert_eq!(out["INCHI"], "");
    }

    #[test]
    fn forme_d_un_smiles() {
        for ok in ["CCO", "c1ccccc1", "C[C@@H](N)C(=O)O", "[Na+].[Cl-]", "OC(=O)C(Cl)Br", "C/C=C\\C", "[nH]1cccc1", "CN1C=NC2=C1C(=O)N(C(=O)N2C)C"] {
            assert!(looks_like_smiles(ok), "{ok}");
        }
        for bad in ["ZZORFUFYDOWNEF-UHFFFAOYAR", "LFQSCWFLJHTTHZ-UHFFFAOYSA-N", "Caffeine", "N/A", "InChI=1S/C2H6O", "C C O", "", "[Na+"] {
            assert!(!looks_like_smiles(bad), "{bad}");
        }
        assert!(crate::globals_vars::INCHI_STRICT_PATTERN.is_match("InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3"));
        assert!(!crate::globals_vars::INCHI_STRICT_PATTERN.is_match("InChI=1S/C10H13N5O5/c11-8 [M-H]-"));
    }

    #[test]
    fn real_misplacements_are_still_repaired() {
        let inchi = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3";
        let key = "LFQSCWFLJHTTHZ-UHFFFAOYSA-N";
        // InChI rangé dans SMILES
        let out = repair_mol_descriptors(meta(inchi, "", key));
        assert_eq!(out["INCHI"], inchi);
        assert_eq!(out["SMILES"], "");
        // InChIKey rangé dans SMILES
        let out = repair_mol_descriptors(meta(key, inchi, ""));
        assert_eq!(out["INCHIKEY"], key);
        assert_eq!(out["SMILES"], "");
        // SMILES rangé dans INCHI
        let out = repair_mol_descriptors(meta("", "CCO(C)CCN", key));
        assert_eq!(out["SMILES"], "CCO(C)CCN");
        assert_eq!(out["INCHI"], "");
    }

    #[test]
    fn inchi_prefix_is_repaired() {
        let out = repair_mol_descriptors(meta("CCO", "1S/C2H6O/c1-2-3/h3H,2H2,1H3", ""));
        assert_eq!(out["INCHI"], "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3");
        let out = repair_mol_descriptors(meta("CCO", "InChI=InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3", ""));
        assert_eq!(out["INCHI"], "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3");
    }

    #[test]
    fn inchikey_field_validation() {
        use crate::globals_vars::INCHIKEY_PATTERN as P;
        assert!(P.is_match("LFQSCWFLJHTTHZ-UHFFFAOYSA-N"));
        assert!(P.is_match(" LFQSCWFLJHTTHZ-UHFFFAOYSA-N "));
        assert!(P.is_match("LFQSCWFLJHTTHZ"));
        assert!(!P.is_match("CCCCCCCCCCCCCCCC(=O)O"));
        assert!(!P.is_match("NOT FOUND LFQSCWFLJHTTHZ-UHFFFAOYSA-N"));
    }
}
