// src/peaks_filters/reduce_peak_list.rs

/// Réduit la liste des pics pour ne conserver que les `max_peaks` les plus intenses.
/// 
/// Pour un développeur Python : Cette fonction équivaut à trier une liste par la valeur de l'intensité 
/// de manière décroissante, couper (slicer) la liste à `max_peaks`, puis la retrier par valeur m/z croissante.
///
/// # Arguments
/// * `peaks` (Vec<(f64, f64)>) : Le vecteur `mut`able de pics.
/// * `max_peaks` (usize) : Le nombre maximum de pics à conserver.
///
/// # Returns
/// * `Vec<(f64, f64)>` : La liste réduite des pics.
pub fn reduce_peak_list(
    mut peaks: Vec<(f64, f64)>,
    max_peaks: usize
) -> Vec<(f64, f64)> {
    if peaks.len() > max_peaks {
        // En Rust, les flottants (`f64`) ne peuvent pas être comparés directement avec `.cmp()` 
        // à cause de la valeur spéciale `NaN` (Not a Number). On utilise `partial_cmp` qui renvoie un `Option`.
        // `unwrap_or(std::cmp::Ordering::Equal)` gère le cas où `NaN` apparaît en les considérant comme égaux.
        // Tri décroissant sur l'intensité (`b.1` vs `a.1`).
        peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // `.truncate()` coupe le vecteur sur place pour ne garder que les N premiers éléments. 
        // C'est l'équivalent de `peaks = peaks[:max_peaks]` en Python, mais sans copier la mémoire.
        peaks.truncate(max_peaks);
        
        // Tri croissant sur la masse m/z (`a.0` vs `b.0`).
        peaks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garde_les_plus_intenses_tries_par_mz() {
        let peaks = vec![(300.0, 5.0), (100.0, 50.0), (200.0, 1.0), (150.0, 30.0)];
        assert_eq!(reduce_peak_list(peaks, 2), vec![(100.0, 50.0), (150.0, 30.0)]);
    }

    #[test]
    fn liste_assez_courte_inchangee_dans_son_ordre() {
        let peaks = vec![(300.0, 5.0), (100.0, 50.0)];
        assert_eq!(reduce_peak_list(peaks.clone(), 2), peaks);
    }
}
