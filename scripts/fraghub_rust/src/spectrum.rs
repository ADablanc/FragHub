use std::collections::HashMap;

/// Structure de données centrale représentant un spectre de masse.
///
/// Pour un développeur Python : C'est l'équivalent d'une `dataclass` ou d'un dictionnaire typé.
/// L'attribut `#[derive(...)]` demande au compilateur Rust de générer 
/// automatiquement pour nous le code pour copier (`.clone()`), afficher (`print!()` / `Debug`), 
/// et créer un spectre vide (`Spectrum::default()`). C'est extrêmement pratique !
#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    pub metadata: HashMap<String, String>,
    pub peaks: Vec<(f64, f64)>, // Toujours des tuples pour éviter la fragmentation RAM
}

/// Texte des pics tel qu'il était gardé dans `metadata["PEAKS_LIST"]` après le nettoyage
/// (`"{:.8} {:.8}"` par pic, séparés par des sauts de ligne). Il n'est plus gardé en mémoire (les pics étaient
/// stockés deux fois) : il est fabriqué seulement là où il est écrit (fichiers DELETED_SPECTRUMS des étapes qui
/// suivent le nettoyage) ou compté (choix du doublon gardé).
/// Nombre de décimales des valeurs CALCULÉES par FragHub (m/z recalculé, masses, temps de rétention converti,
/// intensités ramenées à 1), comme l'entropie. Une valeur lue dans un fichier n'est jamais arrondie.
pub const COMPUTED_DECIMALS: f64 = 1e8;

/// Valeur calculée arrondie à 8 décimales. Avant, le résultat d'une division ou d'une somme était écrit avec tous
/// ses chiffres (« 0.16666666666666666 », « 195.08765155999998 », « 893.6960000000009 »).
/// Une valeur trop grande pour être arrondie ainsi, ou non finie, est rendue telle quelle.
pub fn round_computed(x: f64) -> f64 {
    if !x.is_finite() || x.abs() >= 1e7 {
        return x;
    }
    (x * COMPUTED_DECIMALS).round() / COMPUTED_DECIMALS
}

/// Texte d'une valeur calculée : 8 décimales au plus, sans zéros inutiles (« 195.08765156 », « 1.5 », « 2 »).
pub fn computed_text(x: f64) -> String {
    round_computed(x).to_string()
}

pub fn peaks_text(peaks: &[(f64, f64)]) -> String {
    let mut text = String::with_capacity(peaks.len() * 24);
    for (i, &(mz, int)) in peaks.iter().enumerate() {
        if i > 0 { text.push('\n'); }
        text.push_str(&format!("{:.8} {:.8}", mz, int));
    }
    text
}

/// Texte de `PEAKS_LIST` d'un spectre : celui des métadonnées s'il y est (formules du de novo), sinon celui des pics.
pub fn peaks_list_text(spec: &Spectrum) -> String {
    match spec.metadata.get("PEAKS_LIST") {
        Some(text) => text.clone(),
        None => peaks_text(&spec.peaks),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_des_pics_huit_decimales() {
        assert_eq!(peaks_text(&[(100.5, 1.0), (200.123456789, 0.5)]), "100.50000000 1.00000000\n200.12345679 0.50000000");
        assert_eq!(peaks_text(&[]), "");
    }

    #[test]
    fn valeurs_calculees_huit_decimales_au_plus() {
        assert_eq!(computed_text(1.0 / 6.0), "0.16666667");
        assert_eq!(computed_text(194.08037556 + 1.007276), "195.08765156");      // était 195.08765155999998
        assert_eq!(computed_text(893.6960000000009), "893.696");
        assert_eq!(computed_text(70.99 / 60.0), "1.18316667");
        // Valeurs déjà courtes : inchangées, sans zéros ajoutés
        assert_eq!(computed_text(0.2), "0.2");
        assert_eq!(computed_text(1.5), "1.5");
        assert_eq!(computed_text(2.0), "2");
        assert_eq!(computed_text(-1.007276), "-1.007276");
        assert_eq!(computed_text(1234.56789012), "1234.56789012");
        // Non finie ou très grande : rendue telle quelle
        assert!(round_computed(f64::NAN).is_nan());
        assert_eq!(round_computed(1.23456789012e9), 1.23456789012e9);
    }

    #[test]
    fn texte_des_metadonnees_prioritaire() {
        let mut s = Spectrum { peaks: vec![(100.0, 1.0)], ..Default::default() };
        assert_eq!(peaks_list_text(&s), "100.00000000 1.00000000");
        s.metadata.insert("PEAKS_LIST".into(), "100 1 C5H5N5+/0.1".into());
        assert_eq!(peaks_list_text(&s), "100 1 C5H5N5+/0.1");
    }
}
