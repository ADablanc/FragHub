// src/complete_from_pubchem_datas.rs
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;

/// Complète les spectres avec les données issues de PubChem.
///
/// Pour un développeur Python : Au lieu de faire un `.merge()` extrêmement coûteux
/// en RAM avec Pandas, on itère simplement sur notre liste de spectres (`iter_mut()`).
/// Si l'INCHIKEY du spectre existe dans le dictionnaire `pubchem_datas` (déjà chargé en RAM
/// par `loading_db.rs`), on copie instantanément les valeurs manquantes sans recréer de nouvelle table.
pub fn complete_from_pubchem_datas(
    py: Python,
    mut spectrum_list: Vec<Spectrum>,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<Spectrum>> {

    // --- Step 1: Initialization ---
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Enriching data from PubChem:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("rows",))?; }

    let total_items = spectrum_list.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items, 0))?; }

    // --- Step 2: Récupération de ontologies_datas (qui contient désormais pubchem) depuis Rust Global State ---
    let state = crate::global_state::STATE.read().unwrap();
    let pubchem_dict = &state.ontologies_datas;

    let columns_to_update = ["INCHI", "SMILES", "FORMULA", "NAME", "EXACTMASS", "AVERAGEMASS"];
    let mut processed = 0;
    let chunk_size = 500;

    // --- Step 3: Boucle de mise à jour (Multithreaded) ---
    for chunk in spectrum_list.chunks_mut(chunk_size) {
        py.allow_threads(|| {
            chunk.par_iter_mut().for_each(|spec| {
                let inchikey = spec.metadata.get("INCHIKEY").cloned().unwrap_or_default();

                if !inchikey.is_empty() && inchikey.to_lowercase() != "nan" {
                    if let Some(pubchem_row) = pubchem_dict.get(&inchikey) {
                        for col in &columns_to_update {
                            if let Some(new_val) = pubchem_row.get(*col) {
                                if !new_val.trim().is_empty() && new_val.to_lowercase() != "nan" {
                                    spec.metadata.insert(col.to_string(), new_val.clone());
                                }
                            }
                        }
                    }
                }
            });
        });

        processed += chunk.len();
        if let Some(cb) = &progress_callback { cb.call1(py, (processed,))?; }
    }

    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }

    Ok(spectrum_list)
}