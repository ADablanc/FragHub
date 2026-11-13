// src/peaks_filters/keep_mz_in_range.rs

/// Supprime les pics qui tombent en dehors de la plage m/z [mz_from, mz_to].
///
/// # Arguments
/// * `peaks` (Vec<(f64, f64)>) : Le vecteur `mut`able de pics.
/// * `mz_from` (f64) : Borne inférieure de la masse m/z.
/// * `mz_to` (f64) : Borne supérieure de la masse m/z.
/// * `deletion_reason` (&mut Option<String>) : Référence mutable pour tracker la suppression.
///
/// # Returns
/// * `Vec<(f64, f64)>` : La liste filtrée (vide si tous les pics sont supprimés).
pub fn keep_mz_in_range(
    mut peaks: Vec<(f64, f64)>,
    mz_from: f64,
    mz_to: f64,
    deletion_reason: &mut Option<String>
) -> Vec<(f64, f64)> {
    // Comme pour `remove_peak_above_precursormz`, `.retain()` modifie le vecteur sur place.
    // La condition `mz >= mz_from && mz <= mz_to` est conservée (`true`), le reste est jeté.
    peaks.retain(|&(mz, _)| mz >= mz_from && mz <= mz_to);

    if peaks.is_empty() {
        *deletion_reason = Some("spectrum deleted because peaks list is empty after removing peaks out of mz range choiced by the user".to_string());
    }

    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bornes_incluses() {
        let mut reason = None;
        let out = keep_mz_in_range(vec![(49.9, 1.0), (50.0, 2.0), (100.0, 3.0), (2000.0, 4.0), (2000.1, 5.0)], 50.0, 2000.0, &mut reason);
        assert_eq!(out, vec![(50.0, 2.0), (100.0, 3.0), (2000.0, 4.0)]);
        assert!(reason.is_none());
    }

    #[test]
    fn plus_aucun_pic_spectre_supprime() {
        let mut reason = None;
        assert!(keep_mz_in_range(vec![(10.0, 1.0)], 50.0, 2000.0, &mut reason).is_empty());
        assert!(reason.unwrap().contains("out of mz range"));
    }
}
