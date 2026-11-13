// src/peaks_filters/normalize_intensity.rs

/// Normalise l'intensité des pics de sorte que le pic le plus intense ait une valeur de 1.0.
/// 
/// Pour un développeur Python : En Python on ferait `max_int = max([p[1] for p in peaks])`.
/// En Rust, on utilise un concept fonctionnel puissant : les Itérateurs.
///
/// # Arguments
/// * `peaks` (Vec<(f64, f64)>) : Le vecteur `mut`able de pics.
///
/// # Returns
/// * `Vec<(f64, f64)>` : La liste des pics normalisés.
pub fn normalize_intensity(mut peaks: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if peaks.is_empty() {
        return peaks; // Sortie prématurée avec `return`.
    }

    // 1. `.iter()` : Crée un itérateur sur les éléments (sans les consommer).
    // 2. `.map(|p| p.1)` : Extrait seulement la deuxième valeur (l'intensité).
    // 3. `.fold(0.0_f64, f64::max)` : Équivalent de `reduce` en Python. 
    //    Part de 0.0 et garde à chaque étape le maximum entre l'accumulateur et la valeur courante.
    let max_int = peaks.iter().map(|p| p.1).fold(0.0_f64, f64::max);

    if max_int != 0.0 {
        // On itère avec `&mut` (référence mutable) pour modifier chaque pic in-place.
        for p in &mut peaks {
            // Intensité relative arrondie à 8 décimales (« 0.16666667 » et non « 0.16666666666666666 ») : la valeur
            // gardée en mémoire est celle qui est écrite, donc l'entropie et le SPLASH sont ceux du spectre écrit.
            // Un pic très faible n'est jamais arrondi à zéro (il garde alors sa valeur exacte).
            let relative = p.1 / max_int;
            let rounded = crate::spectrum::round_computed(relative);
            p.1 = if rounded > 0.0 { rounded } else { relative };
        }
        peaks
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pic_le_plus_intense_vaut_1() {
        assert_eq!(normalize_intensity(vec![(50.0, 50.0), (60.0, 200.0), (70.0, 100.0)]),
                   vec![(50.0, 0.25), (60.0, 1.0), (70.0, 0.5)]);
    }

    #[test]
    fn intensites_arrondies_a_huit_decimales() {
        let out = normalize_intensity(vec![(50.0, 10.0), (60.0, 60.0), (70.0, 24.7102), (80.0, 100.0)]);
        // 10/60 = 0.1666… ; 24.7102/100 donnait 0.24710200000000002
        let int: Vec<String> = out.iter().map(|p| p.1.to_string()).collect();
        assert_eq!(int, ["0.1", "0.6", "0.247102", "1"]);
        assert_eq!(normalize_intensity(vec![(50.0, 1.0), (60.0, 6.0)])[0].1.to_string(), "0.16666667");
        // Pic un milliard de fois plus faible que le plus intense : pas arrondi à zéro
        let tiny = normalize_intensity(vec![(50.0, 1.0), (60.0, 1e9)]);
        assert_eq!(tiny[0].1, 1e-9);
    }

    #[test]
    fn intensites_toutes_nulles_ou_liste_vide() {
        assert!(normalize_intensity(vec![(50.0, 0.0), (60.0, 0.0)]).is_empty());
        assert!(normalize_intensity(Vec::new()).is_empty());
    }
}
