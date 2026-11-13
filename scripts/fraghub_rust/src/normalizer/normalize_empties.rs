// src/normalizer/normalize_empties.rs
use std::collections::HashMap;

/// Standardise les valeurs vides ou "nulles" du dictionnaire en chaînes vides ("").
/// 
/// Pour un développeur Python : En Rust, un `HashMap<String, String>` (l'équivalent de `Dict[str, str]`) 
/// ne permet pas qu'une valeur soit `None`. Tout est une chaîne de caractères `String`.
/// On parcourt donc les valeurs et on les remplace par `""` (String vide).
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Dictionnaire contenant les métadonnées.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire nettoyé des valeurs nulles/vides.
pub fn normalize_empties(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {

    // `.values_mut()` crée un itérateur qui permet de modifier (muter) directement 
    // les valeurs du dictionnaire sans toucher aux clés. C'est plus propre que `for (k, v) in dict.items()` en Python.
    for v in metadata_dict.values_mut() {
        if crate::globals_vars::EMPTY_PATTERN.is_match(v) {
            // L'étoile `*` déréférence la référence mutable `&mut String` pour remplacer son contenu.
            // `String::new()` crée une chaîne vide allouée dynamiquement sans garbage collector.
            *v = String::new(); 
        }
        // NB: Pas besoin de vérifier f64::NAN ici !
        // En Rust, tout est déjà String et le Regex `EMPTY_PATTERN` attrape déjà "nan".
    }

    metadata_dict // Retourne la propriété (ownership) du dictionnaire modifié.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_vides_ecrites_remplacees_par_rien() {
        let mut m = HashMap::new();
        for (k, v) in [("a", "nan"), ("b", "N/A"), ("c", "unknown"), ("d", "0"), ("e", "$:00in-source"), ("f", "NA"),
                       ("g", "?"), ("h", "None"), ("i", "")] {
            m.insert(k.to_string(), v.to_string());
        }
        let out = normalize_empties(m);
        assert!(out.values().all(|v| v.is_empty()), "{out:?}");
    }

    #[test]
    fn vraies_valeurs_gardees() {
        let mut m = HashMap::new();
        for (k, v) in [("a", "NaCl"), ("b", "10"), ("c", "Unknown compound 3"), ("d", "x2")] {
            m.insert(k.to_string(), v.to_string());
        }
        let out = normalize_empties(m.clone());
        assert_eq!(out["a"], "NaCl");
        assert_eq!(out["b"], "10");
        assert_eq!(out["d"], "x2");
        // « unknown » suivi d'un espace et d'autre chose est aussi lu comme vide (règle EMPTY_PATTERN)
        assert_eq!(out["c"], "");
    }
}
