// src/deletion_report.rs

/// Structure permettant de compter les raisons de suppression des spectres invalides.
///
/// Pour un développeur Python : Au lieu de trimballer un dictionnaire pour les compteurs
/// (ce qui obligerait à hasher des clés à chaque incrémentation, ralentissant le programme),
/// on définit une structure fortement typée avec des compteurs de type `usize` (entiers positifs liés à l'architecture).
/// `Default` permet d'initialiser tous ces compteurs à zéro automatiquement lors de la création.
#[derive(Clone, Default, Debug)]
pub struct DeletionReport {
    pub duplicatas_removed: usize,
    pub previously_cleaned: usize,
    pub no_peaks_list: usize,
    pub no_smiles_no_inchi_no_inchikey: usize,
    pub no_precursor_mz: usize,
    pub low_entropy_score: usize,
    pub minimum_peaks_not_requiered: usize,
    pub all_peaks_above_precursor_mz: usize,
    pub no_peaks_in_mz_range: usize,
    pub minimum_high_peaks_not_requiered: usize,
    pub no_or_bad_adduct: usize,
    pub low_resolution_ms2: usize,
    pub ms2_chemical_crash: usize,
}
