// src/normalizer/delete_no_smiles_no_inchi.rs
use std::collections::HashMap;

/// Supprime un spectre si les métadonnées n'ont aucun des trois identifiants chimiques.
///
/// Pour un développeur Python : En Rust, pour indiquer qu'une fonction peut échouer ou retourner "Rien", 
/// on encapsule le retour dans un enum `Option<T>`. On renvoie `Some(valeur)` si c'est valide, et `None` sinon.
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Dictionnaire des métadonnées.
/// * `deletion_reason` (&mut Option<String>) : Pointeur mutable pour indiquer la suppression.
///
/// # Returns
/// * `Option<HashMap<String, String>>` : `Some(dict)` si un identifiant existe, sinon `None`.
pub fn delete_no_smiles_no_inchi_no_inchikey(
    metadata_dict: HashMap<String, String>,
    deletion_reason: &mut Option<String>
) -> Option<HashMap<String, String>> {

    // On vérifie si les clés existent ET si elles ne sont pas vides.
    // `.map_or(false, closure)` : si le champ n'existe pas, on renvoie `false`. 
    // Sinon, on applique la fonction anonyme `|s| !s.is_empty()` pour vérifier si le texte n'est pas vide.
    let has_smiles = metadata_dict.get("SMILES").map_or(false, |s| !s.is_empty());
    let has_inchi = metadata_dict.get("INCHI").map_or(false, |s| !s.is_empty());
    let has_inchikey = metadata_dict.get("INCHIKEY").map_or(false, |s| !s.is_empty());

    if !has_smiles && !has_inchi && !has_inchikey {
        // On modifie la variable pointée par `deletion_reason` pour garder une trace de la suppression.
        *deletion_reason = Some("spectrum deleted because it has neither inchi nor smiles nor inchikey".to_string());
        return None; // Le dictionnaire disparaît ici, sa mémoire est libérée.
    }

    // Si au moins un identifiant est présent, on "emballe" le dictionnaire valide dans `Some`.
    Some(metadata_dict)
}

/// Motif de suppression des spectres « suspects » (GNPS) : leur structure est celle d'une molécule apparentée,
/// pas celle de la molécule mesurée.
pub const SUSPECT_REASON: &str = "spectrum deleted because its structure is that of a related compound, not of the measured molecule (suspect)";

static MASS_OFFSET: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"(?i)\s[+-]\s*\d+(?:\.\d+)?\s*Da\s*\W*$").unwrap());

/// Supprime les spectres « suspects » : nom "Suspect related to ..." (GNPS, molécule inconnue apparentée à une
/// molécule connue, "with delta m/z -14.016"), ou structure suivie d'un décalage de masse
/// ("InChI=1S/... - 14.016 Da"). La structure donnée n'est pas celle du spectre : FragHub l'aurait annoté avec
/// la molécule connue (JSON / MSP GNPS, par InChIKey_smiles), alors que le MGF (sans InChIKey) le supprimait.
pub fn delete_suspect(
    metadata_dict: HashMap<String, String>,
    deletion_reason: &mut Option<String>
) -> Option<HashMap<String, String>> {
    let name_suspect = metadata_dict.get("NAME")
        .map_or(false, |n| n.trim().trim_matches('"').trim().to_lowercase().starts_with("suspect related to"));
    let offset = ["INCHI", "SMILES"].iter()
        .any(|k| metadata_dict.get(*k).map_or(false, |v| MASS_OFFSET.is_match(v)));
    if name_suspect || offset {
        *deletion_reason = Some(SUSPECT_REASON.to_string());
        return None;
    }
    Some(metadata_dict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, inchi: &str) -> HashMap<String, String> {
        [("NAME", name), ("INCHI", inchi), ("SMILES", "")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn suspects_supprimes() {
        let mut r = None;
        // Cas réels GNPS (CCMSLIB00012162089)
        assert!(delete_suspect(m("Suspect related to Thr-Val-Leu (predicted molecular formula SIRIUS: C14H27N3O5) with delta m/z -14.016", "InChI=1S/C15H29N3O5/c1 - 14.016 Da"), &mut r).is_none());
        assert_eq!(r.as_deref(), Some(SUSPECT_REASON));
        assert!(delete_suspect(m("Truc", "\"InChI=1S/C22H20O11/c1-30 + 18.0106 Da\""), &mut None).is_none());
        // Spectres ordinaires gardés (adduit, nom avec « suspect » ailleurs, InChI normal)
        assert!(delete_suspect(m("Quercetin", "InChI=1S/C15H10O7/c16-7-4-10(19)"), &mut None).is_some());
        assert!(delete_suspect(m("Quercetin", "InChI=1S/C15H10O7/c16 [M-H]-"), &mut None).is_some());
        assert!(delete_suspect(m("Not a suspect related to anything", "InChI=1S/C2H6O/c1-2-3"), &mut None).is_some());
        assert!(delete_suspect(m("Candidate Trihydroxylated bile acid", "InChI=1S/C24H40O5/c1"), &mut None).is_some());
    }

    #[test]
    fn sans_aucun_identifiant_supprime() {
        let fields = |f: &[(&str, &str)]| -> HashMap<String, String> { f.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() };
        let mut r = None;
        // Ni SMILES, ni InChI, ni InChIKey (champs absents ou vides) : supprimé avec son motif
        assert!(delete_no_smiles_no_inchi_no_inchikey(fields(&[("NAME", "X")]), &mut r).is_none());
        assert_eq!(r.as_deref(), Some("spectrum deleted because it has neither inchi nor smiles nor inchikey"));
        assert!(delete_no_smiles_no_inchi_no_inchikey(fields(&[("SMILES", ""), ("INCHI", ""), ("INCHIKEY", "")]), &mut None).is_none());
        // Un seul des trois suffit : gardé tel quel, sans motif
        for key in ["SMILES", "INCHI", "INCHIKEY"] {
            let mut r = None;
            let kept = delete_no_smiles_no_inchi_no_inchikey(fields(&[("NAME", "X"), (key, "valeur")]), &mut r).expect("gardé");
            assert_eq!(kept[key], "valeur");
            assert!(r.is_none());
        }
    }
}
