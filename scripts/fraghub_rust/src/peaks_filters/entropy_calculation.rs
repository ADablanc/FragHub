// src/peaks_filters/entropy_calculation.rs

/// Calcule la "normalized entropy" d'un spectre (Li et al. 2021, Nature Methods -
/// convention utilisée par MoNA/MassBank), bornee entre 0 et 1.
///
/// Pour un développeur Python : C'est l'équivalent exact de votre fonction optimisée avec `@jit(nopython=True)` via Numba.
/// En Rust, on passe un `&[f64]` (une *slice*, ou portion de tableau lue en lecture seule).
/// C'est plus léger que de passer un `Vec` car ça ne nécessite pas de posséder la mémoire.
///
/// # Formule
/// * `p_i = I_i / sum(I_j)`               : normalisation des intensités.
/// * `S = - sum(p_i * ln(p_i))`           : spectral entropy (en nats, logarithme NEPERIEN).
/// * `S_norm = S / ln(N)`                 : normalized entropy (N = nombre de pics), bornee [0, 1].
///
/// ⚠️ Avant correction, cette fonction utilisait `log2` (bits, base 2) au lieu de `ln`
/// (nats, base e) et ne divisait jamais par `ln(N)` : elle renvoyait donc l'entropie
/// brute (non bornee, grandissant avec le nombre de pics) au lieu de la "normalized
/// entropy" [0, 1] attendue par le seuil de l'interface (`remove_spectrum_under_entropy_score_value`,
/// dont la valeur par défaut est 0.5 — cohérent uniquement avec une echelle 0-1).
///
/// # Arguments
/// * `peak_intensities` (&[f64]) : Tableau contenant uniquement les intensités des pics.
///
/// # Returns
/// * `f64` : La normalized entropy (0.0 a 1.0).
pub fn entropy_calculation(peak_intensities: &[f64]) -> f64 {
    // On ne garde que les intensités strictement positives (un pic à 0 ne participe
    // ni à la somme totale, ni au décompte N, ni à l'entropie).
    let total_intensity: f64 = peak_intensities.iter().filter(|&&i| i > 0.0).sum();

    if total_intensity <= 0.0 {
        return 0.0;
    }

    // En Rust, toute variable qui va être modifiée doit être explicitement déclarée `mut`.
    let mut entropy = 0.0;
    let mut n_peaks: usize = 0;

    // Itération sur la slice. `&intensity` permet de récupérer la valeur f64 sans déréférencer manuellement.
    for &intensity in peak_intensities {
        if intensity > 0.0 {
            let prob = intensity / total_intensity;
            // Equivalent strict de `-np.sum(probabilities * np.log(probabilities))` (logarithme naturel).
            entropy -= prob * prob.ln();
            n_peaks += 1;
        }
    }

    // Impossible de normaliser avec 0 ou 1 seul pic (ln(1) = 0, ln(0) indéfini) :
    // on retourne 0.0 comme le fait l'implémentation de référence.
    if n_peaks <= 1 {
        return 0.0;
    }

    entropy / (n_peaks as f64).ln() // normalized entropy, bornee [0, 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proche(a: f64, b: f64) -> bool { (a - b).abs() < 1e-12 }

    #[test]
    fn pics_de_meme_intensite_entropie_normalisee_1() {
        assert!(proche(entropy_calculation(&[5.0, 5.0]), 1.0));
        assert!(proche(entropy_calculation(&[1.0, 1.0, 1.0, 1.0]), 1.0));
    }

    #[test]
    fn valeur_de_reference() {
        // p = (1/3, 2/3) : S = -(1/3 ln 1/3 + 2/3 ln 2/3) = 0.636514..., divisée par ln 2
        let s = -(1.0_f64 / 3.0 * (1.0_f64 / 3.0).ln() + 2.0 / 3.0 * (2.0_f64 / 3.0).ln());
        assert!(proche(entropy_calculation(&[1.0, 2.0]), s / 2.0_f64.ln()));
        assert!((entropy_calculation(&[1.0, 2.0]) - 0.918296).abs() < 1e-6);
    }

    #[test]
    fn un_pic_ou_aucun_vaut_0_et_pics_nuls_ignores() {
        assert_eq!(entropy_calculation(&[]), 0.0);
        assert_eq!(entropy_calculation(&[10.0]), 0.0);
        assert_eq!(entropy_calculation(&[0.0, -1.0]), 0.0);
        assert!(proche(entropy_calculation(&[5.0, 0.0, 5.0]), 1.0));   // le pic nul ne compte pas dans N
    }

    #[test]
    fn toujours_entre_0_et_1() {
        let e = entropy_calculation(&[1000.0, 1.0, 1.0, 1.0]);
        assert!(e > 0.0 && e < 0.2);
    }
}
