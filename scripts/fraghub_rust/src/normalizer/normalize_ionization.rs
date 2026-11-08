// src/normalizer/normalize_ionization.rs
use std::collections::HashMap;

/// Normalise le mode d'ionisation en lisant IONIZATION ou INSTRUMENTTYPE.
///
/// Pour un développeur Python : Cette fonction illustre le "Pattern Matching" avec `if let`.
/// C'est une façon très élégante de dire "Si ça contient quelque chose, extrait-le et exécute ce bloc".
///
/// # Arguments
/// * `metadata_dict` (HashMap<String, String>) : Le dictionnaire des métadonnées du spectre.
///
/// # Returns
/// * `HashMap<String, String>` : Le dictionnaire avec l'ionisation standardisée.
pub fn normalize_ionization(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let mut found_mode: Option<String> = None;

    // Valeur d'origine gardée pour la recherche d'instrument (étape suivante), qui ne verrait sinon que la
    // valeur réécrite : GNPS JSON "Ion_Source: LC-ESI" devenait "ESI", le LC était perdu ("ESI-FT" au lieu
    // de "LC-ESI-FT" comme le MSP GNPS). Clé interne INSTRUMENT_HINTS, jamais écrite.
    if let Some(raw) = metadata_dict.get("IONIZATION").map(|s| s.trim().to_string()) {
        if !raw.is_empty() {
            let e = metadata_dict.entry(crate::globals_vars::INSTRUMENT_HINTS_KEY.to_string()).or_default();
            if !e.is_empty() { e.push(' '); }
            e.push_str(&raw);
        }
    }

    // 1. Chercher d'abord dans IONIZATION
    // On extrait la valeur. Si elle n'existe pas, le bloc est ignoré sans générer d'erreur KeyError.
    if let Some(ion) = metadata_dict.get("IONIZATION") {
        if let Some(caps) = crate::globals_vars::IONIZATION_MODE_PATTERN.captures(ion) {
            // Les groupes de l'expression régulière (capture) retournent aussi des Option.
            if let Some(m) = caps.get(1) {
                // On met en majuscules pour garantir la correction (ex: "apci" -> "APCI").
                found_mode = Some(m.as_str().to_uppercase());
            }
        }
    }

    // 2. Si rien n'est trouvé, chercher dans INSTRUMENTTYPE
    // `.is_none()` équivaut à `found_mode is None` en Python.
    if found_mode.is_none() {
        if let Some(inst) = metadata_dict.get("INSTRUMENTTYPE") {
            if let Some(caps) = crate::globals_vars::IONIZATION_MODE_PATTERN.captures(inst) {
                if let Some(m) = caps.get(1) {
                    found_mode = Some(m.as_str().to_uppercase());
                }
            }
        }
    }

    // 3. Application et correction
    // `mut mode` permet de modifier la variable locale `mode` extraite du `Some()`.
    if let Some(mut mode) = found_mode {
        // Correction de la faute de frappe commune "ACPI" au lieu de "APCI"
        if mode == "ACPI" {
            mode = "APCI".to_string(); // Doit être une String, pas un &str
        }
        metadata_dict.insert("IONIZATION".to_string(), mode);
    } else {
        // S'il n'y avait rien, on s'assure que le champ existe et est vide.
        metadata_dict.insert("IONIZATION".to_string(), String::new());
    }

    metadata_dict
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ionisation_d_origine_gardee_pour_l_instrument() {
        // GNPS JSON : "Ion_Source: LC-ESI" -> IONIZATION "ESI", mais "LC-ESI" reste visible pour l'instrument
        let m: HashMap<String, String> = [("IONIZATION".to_string(), "LC-ESI".to_string()),
            (crate::globals_vars::INSTRUMENT_HINTS_KEY.to_string(), "".to_string())].into_iter().collect();
        let m = normalize_ionization(m);
        assert_eq!(m["IONIZATION"], "ESI");
        assert_eq!(m[crate::globals_vars::INSTRUMENT_HINTS_KEY], "LC-ESI");
        // Indices déjà présents : mis bout à bout
        let m: HashMap<String, String> = [("IONIZATION".to_string(), "LC-ESI".to_string()),
            (crate::globals_vars::INSTRUMENT_HINTS_KEY.to_string(), "Triple_Quad".to_string())].into_iter().collect();
        assert_eq!(normalize_ionization(m)[crate::globals_vars::INSTRUMENT_HINTS_KEY], "Triple_Quad LC-ESI");
        // Pas d'ionisation : pas d'indice ajouté
        let m: HashMap<String, String> = [("INSTRUMENTTYPE".to_string(), "LC-ESI-QTOF".to_string())].into_iter().collect();
        let m = normalize_ionization(m);
        assert_eq!(m["IONIZATION"], "ESI");
        assert!(!m.contains_key(crate::globals_vars::INSTRUMENT_HINTS_KEY));
    }
}
