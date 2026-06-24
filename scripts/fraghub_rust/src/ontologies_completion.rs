// src/ontologies_completion.rs
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;

/// Ajoute les classes chimiques (ClassyFire et NPClassifier) aux spectres.
///
/// Pour un développeur Python : La même logique ultra-rapide que pour PubChem s'applique ici.
/// Pas de jointures SQL ou Pandas. Un simple "Lookup" (recherche) en `O(1)` dans le dictionnaire
/// des ontologies. Si l'INCHIKEY correspond, on transfère la classe chimique.
pub fn ontologies_completion_processing(
    py: Python,
    mut spectrum_list: Vec<Spectrum>,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<Spectrum>> {

    // --- Step 1: Initialization ---
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Updating ontologies:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("rows",))?; }

    // On sauvegarde l'ordre des colonnes


    // Contrairement à votre Python qui simulait la progression avec le nombre de clés uniques,
    // on va faire une vraie progression fluide basée sur le nombre total de lignes.
    let total_items = spectrum_list.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items, 0))?; }

    // --- Step 2: Récupération de ontologies_df depuis Rust Global State ---
    let state = crate::global_state::STATE.read().unwrap();
    let ont_dict = &state.ontologies_datas;

    let columns_to_update = [
        "CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS",
        "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS"
    ];

    let mut processed = 0;
    let chunk_size = 500;

    // --- Step 3: Boucle de mise à jour (Multithreaded) ---
    for chunk in spectrum_list.chunks_mut(chunk_size) {
        py.allow_threads(|| {
            chunk.par_iter_mut().for_each(|spec| {
                // Par défaut, on initialise tout à "NOT FOUND" comme dans votre code Python
                for col in &columns_to_update {
                    spec.metadata.insert(col.to_string(), "NOT FOUND".to_string());
                }

                // Si on a un INCHIKEY valide, on cherche dans la base de données
                let inchikey = spec.metadata.get("INCHIKEY").cloned().unwrap_or_default();

                if !inchikey.is_empty() && inchikey.to_lowercase() != "nan" {
                    if let Some(ont_row) = ont_dict.get(&inchikey) {
                        for col in &columns_to_update {
                            if let Some(new_val) = ont_row.get(*col) {
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
        // Barre de progression
        if let Some(cb) = &progress_callback { cb.call1(py, (processed,))?; }
    }

    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }

    Ok(spectrum_list)
}