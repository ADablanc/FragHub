// src/peaks_filters/check_minimum_of_high_peaks_requiered.rs

/// Vérifie qu'un nombre minimum de pics d'une certaine intensité est présent.
/// 
/// Pour un développeur Python : Cette fonction combine le calcul d'un max, un calcul de seuil, 
/// et un comptage conditionnel, tout en évitant les boucles manuelles grâce aux itérateurs.
///
/// # Arguments
/// * `peaks` (Vec<(f64, f64)>) : La liste de pics du spectre.
/// * `intensity_percent` (f64) : Le pourcentage de l'intensité maximale requis comme seuil.
/// * `no_peaks` (usize) : Le nombre minimum de pics devant dépasser ce seuil.
/// * `deletion_reason` (&mut Option<String>) : Pointeur mutable pour inscrire la raison si le spectre est rejeté.
///
/// # Returns
/// * `Vec<(f64, f64)>` : La liste de pics (vide si la condition n'est pas remplie).
pub fn check_minimum_of_high_peaks_requiered(
    peaks: Vec<(f64, f64)>,
    intensity_percent: f64,
    no_peaks: usize,
    deletion_reason: &mut Option<String>
) -> Vec<(f64, f64)> {
    if peaks.is_empty() {
        return peaks;
    }

    // Trouve l'intensité maximale dans le spectre.
    let max_int = peaks.iter().map(|p| p.1).fold(0.0_f64, f64::max);
    
    // Calcule le seuil absolu basé sur le pourcentage passé en argument.
    let threshold = max_int * (intensity_percent / 100.0);

    // `.iter()` parcourt la liste.
    // `.filter()` ne garde que les éléments dépassant le seuil (Closure qui retourne `true`).
    // `.count()` compte finalement le nombre d'éléments filtrés. (Très rapide et sans allocation mémoire).
    let high_peaks_count = peaks.iter().filter(|&&(_, int)| int >= threshold).count();

    if high_peaks_count < no_peaks {
        *deletion_reason = Some("spectrum deleted because peaks list does not contain minimum number of high peaks required according to the value choiced by the user".to_string());
        Vec::new() // Spectre jeté, on renvoie un vecteur vide.
    } else {
        peaks // Spectre valide, on le renvoie tel quel.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assez_de_pics_intenses_spectre_garde() {
        // seuil 10 % de 200 = 20 : 3 pics atteignent le seuil (le pic à 20 compte)
        let peaks = vec![(50.0, 200.0), (60.0, 20.0), (70.0, 19.9), (80.0, 100.0)];
        let mut reason = None;
        assert_eq!(check_minimum_of_high_peaks_requiered(peaks.clone(), 10.0, 3, &mut reason), peaks);
        assert!(reason.is_none());
    }

    #[test]
    fn pas_assez_de_pics_intenses_spectre_supprime() {
        let mut reason = None;
        let out = check_minimum_of_high_peaks_requiered(vec![(50.0, 200.0), (60.0, 5.0)], 10.0, 2, &mut reason);
        assert!(out.is_empty());
        assert!(reason.unwrap().contains("minimum number of high peaks"));
    }

    #[test]
    fn liste_vide_inchangee_sans_motif() {
        let mut reason = None;
        assert!(check_minimum_of_high_peaks_requiered(Vec::new(), 10.0, 2, &mut reason).is_empty());
        assert!(reason.is_none());
    }
}
