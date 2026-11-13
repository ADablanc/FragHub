use std::collections::HashMap;
use regex::Regex;
use once_cell::sync::Lazy;

static IE_EI_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(IE|EI)\b").unwrap());

/// Masse exacte de la molécule (InChI de préférence, sinon SMILES), calculée par RDKit C++ (src/rdkit_native).
///
/// # Arguments
/// * `inchi` (&str) : Chaîne de caractères InChI.
/// * `smiles` (&str) : Chaîne de caractères SMILES.
///
/// # Returns
/// * `Option<f64>` : Masse exacte calculée par RDKit, ou `None` si échec.
fn get_exact_mass_from_rdkit(inchi: &str, smiles: &str) -> Option<f64> {
    // Sans verrou Python, donc en parallèle sur tous les cœurs.
    crate::rdkit_native::exact_mass(inchi, smiles)
}

/// Recalcule la valeur PRECURSORMZ si elle est manquante en utilisant la masse de RDKit et le delta de l'adduct.
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Le dictionnaire des métadonnées du spectre.
/// * `context` (&super::NormalizerContext) : Contexte global pour récupérer le delta de masse de l'adduct.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire avec le `PRECURSORMZ` recalculé si nécessaire.
pub fn missing_precursormz_re_calculation(
    mut metadata_dict: HashMap<String, String>,
    context: &super::NormalizerContext
) -> HashMap<String, String> {

    // 1. Vérifier si on a besoin de recalculer
    let pmz_str = metadata_dict.get("PRECURSORMZ").cloned().unwrap_or_default();
    let mut needs_recalc = false;

    if pmz_str.is_empty() {
        needs_recalc = true;
    } else if let Some(caps) = crate::globals_vars::FLOAT_CHECK_PATTERN.captures(&pmz_str) {
        // En Rust, pas besoin de `try/except ValueError` lourd, on utilise `parse::<f64>()` qui renvoie un `Result`.
        let matched_str = caps.get(1).unwrap().as_str().replace(',', ".");
        if let Ok(val) = matched_str.parse::<f64>() {
            if val <= 0.0 { needs_recalc = true; }
        } else { needs_recalc = true; }
    } else {
        needs_recalc = true;
    }

    if !needs_recalc {
        return metadata_dict; // Pas besoin de calcul, on renvoie directement
    }

    // 2. Extraire les infos pour la différence de masse
    let adduct = metadata_dict.get("PRECURSORTYPE").cloned().unwrap_or_default();
    let inst_type = metadata_dict.get("INSTRUMENTTYPE").cloned().unwrap_or_default();

    if IE_EI_PATTERN.is_match(&inst_type) && adduct.is_empty() {
        return metadata_dict;
    }

    // Chaînage `.or_else()` : cherche dans le dico Positif, et si absent, cherche dans le dico Négatif.
    let mass_diff = context.adduct_massdiff_pos.get(&adduct)
        .or_else(|| context.adduct_massdiff_neg.get(&adduct))
        .copied();

    // 3. Calculer avec RDKit (via PyO3) si une différence de masse est trouvée
    if let Some(diff) = mass_diff {
        let inchi = metadata_dict.get("INCHI").cloned().unwrap_or_default();
        let smiles = metadata_dict.get("SMILES").cloned().unwrap_or_default();

        if let Some(em) = get_exact_mass_from_rdkit(&inchi, &smiles) {
            // m/z = (n·M + Δ) / z : gère les multimères ([2M+H]+) et les ions multichargés ([M+2H]2+).
            let new_pmz = crate::adduct_mass_calculator::precursor_mz_from_exact_mass(em, diff, &adduct);
            // 8 décimales au plus : « 195.08765156 » et non « 195.08765155999998 »
            metadata_dict.insert("PRECURSORMZ".to_string(), crate::spectrum::computed_text(new_pmz));
        }
    }

    metadata_dict
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAFFEINE_SMILES: &str = "CN1C=NC2=C1C(=O)N(C(=O)N2C)C";
    const CAFFEINE_INCHI: &str = "InChI=1S/C8H10N4O2/c1-10-4-9-6-5(10)7(13)12(3)8(14)11(6)2/h4H,1-3H3";
    const CAFFEINE_MASS: f64 = 194.0803756;

    /// Différences de masse de datas/adduct_to_convert.csv
    fn context() -> super::super::NormalizerContext {
        let pos = [("[M+H]+", 1.007276), ("[M+2H]2+", 2.014552), ("[2M+H]+", 1.007276), ("[M+Na]+", 22.989221)];
        super::super::NormalizerContext {
            adduct_pos: HashMap::new(), adduct_neg: HashMap::new(),
            adduct_massdiff_pos: pos.iter().map(|(a, d)| (a.to_string(), *d)).collect(),
            adduct_massdiff_neg: [("[M-H]-".to_string(), -1.007276)].into_iter().collect(),
            instrument_tree: serde_json::Value::Null,
        }
    }

    fn recalculated(fields: &[(&str, &str)]) -> String {
        let m: HashMap<String, String> = fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        missing_precursormz_re_calculation(m, &context()).get("PRECURSORMZ").cloned().unwrap_or_default()
    }

    fn close(value: &str, expected: f64) -> bool {
        (value.parse::<f64>().unwrap_or(f64::NAN) - expected).abs() < 1e-4
    }

    #[test]
    fn mz_absent_ou_invalide_recalcule() {
        // m/z = (n·M + différence de masse de l'adduit) / charge
        for missing in ["", "0", "-5", "abc", "0,0"] {
            let v = recalculated(&[("PRECURSORMZ", missing), ("PRECURSORTYPE", "[M+H]+"), ("SMILES", CAFFEINE_SMILES)]);
            assert!(close(&v, CAFFEINE_MASS + 1.007276), "{missing:?} -> {v}");
        }
        // Écrit avec 8 décimales au plus (était 195.08765155999998)
        assert_eq!(recalculated(&[("PRECURSORTYPE", "[M+H]+"), ("SMILES", CAFFEINE_SMILES)]), "195.08765156");
        assert!(close(&recalculated(&[("PRECURSORTYPE", "[M-H]-"), ("SMILES", CAFFEINE_SMILES)]), CAFFEINE_MASS - 1.007276));
        assert!(close(&recalculated(&[("PRECURSORTYPE", "[M+Na]+"), ("SMILES", CAFFEINE_SMILES)]), CAFFEINE_MASS + 22.989221));
        // Ion deux fois chargé, dimère
        assert!(close(&recalculated(&[("PRECURSORTYPE", "[M+2H]2+"), ("SMILES", CAFFEINE_SMILES)]), (CAFFEINE_MASS + 2.014552) / 2.0));
        assert!(close(&recalculated(&[("PRECURSORTYPE", "[2M+H]+"), ("SMILES", CAFFEINE_SMILES)]), 2.0 * CAFFEINE_MASS + 1.007276));
    }

    #[test]
    fn inchi_preferee_au_smiles() {
        // InChI de la caféine, SMILES d'une autre molécule (éthanol) : la masse est celle de l'InChI
        let v = recalculated(&[("PRECURSORTYPE", "[M+H]+"), ("INCHI", CAFFEINE_INCHI), ("SMILES", "CCO")]);
        assert!(close(&v, CAFFEINE_MASS + 1.007276), "{v}");
    }

    #[test]
    fn mz_valide_jamais_modifie() {
        // Même faux pour cette molécule : la valeur du fichier n'est pas remplacée ici (voir mz_correction.rs)
        for given in ["195.0877", "195,0877", "500.1234", "1e2"] {
            assert_eq!(recalculated(&[("PRECURSORMZ", given), ("PRECURSORTYPE", "[M+H]+"), ("SMILES", CAFFEINE_SMILES)]), given);
        }
    }

    #[test]
    fn rien_pour_calculer_champ_laisse_tel_quel() {
        // Adduit inconnu ou absent, structure absente ou illisible : pas de valeur inventée
        assert_eq!(recalculated(&[("PRECURSORMZ", ""), ("PRECURSORTYPE", "[M+Foo]+"), ("SMILES", CAFFEINE_SMILES)]), "");
        assert_eq!(recalculated(&[("PRECURSORMZ", ""), ("PRECURSORTYPE", ""), ("SMILES", CAFFEINE_SMILES)]), "");
        assert_eq!(recalculated(&[("PRECURSORMZ", ""), ("PRECURSORTYPE", "[M+H]+")]), "");
        assert_eq!(recalculated(&[("PRECURSORMZ", "abc"), ("PRECURSORTYPE", "[M+H]+"), ("SMILES", "C1CC(")]), "abc");
        // Ionisation électronique sans adduit (GC) : pas de précurseur à calculer
        assert_eq!(recalculated(&[("PRECURSORMZ", ""), ("PRECURSORTYPE", ""), ("INSTRUMENTTYPE", "GC-EI-TOF"), ("SMILES", CAFFEINE_SMILES)]), "");
    }
}
