// src/mz_correction.rs
use pyo3::prelude::*;
use std::collections::HashMap;
use rayon::prelude::*;

use crate::deletion_report::DeletionReport;
use crate::adduct_mass_calculator;
use crate::spectrum::Spectrum;

/// Étape autonome de l'orchestrateur (STEP 6.5) : Mass & Adduct Correction
///
/// Recalcule la masse exacte et corrige l'adduit si la masse expérimentale dévie trop.
pub fn mz_correction_processing(
    py: Python,
    spectrum_list: Vec<Spectrum>,
    deletion_report: &mut DeletionReport,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<Spectrum>> {
    
    let total_items = spectrum_list.len();
    if let Some(cb) = &total_items_callback {
        cb.call1(py, (total_items,))?;
    }
    if let Some(cb) = &prefix_callback {
        cb.call1(py, ("Mass & Adduct Correction",))?;
    }
    if let Some(cb) = &item_type_callback {
        cb.call1(py, ("spectra",))?;
    }
    if let Some(cb) = &progress_callback {
        cb.call1(py, (0,))?;
    }

    let (adduct_massdiff_pos, adduct_massdiff_neg) = {
        let state = crate::global_state::STATE.read().unwrap();
        (state.adduct_massdiff_dict_pos.clone(), state.adduct_massdiff_dict_neg.clone())
    };

    let batch_size = 1000;
    let mut all_processed = Vec::with_capacity(total_items);
    let mut total_processed = 0;
    
    let chunks: Vec<_> = spectrum_list.chunks(batch_size).collect();
    
    for chunk in chunks {
        py.allow_threads(|| {
            let processed_chunk: Vec<Option<Spectrum>> = chunk.par_iter().map(|spec| {
                let mut new_spec = spec.clone();
                
                let formula = new_spec.metadata.get("FORMULA_RDKit").cloned().unwrap_or_default();
                let precursor_type = new_spec.metadata.get("PRECURSORTYPE").cloned().unwrap_or_default();
                let ion_mode = new_spec.metadata.get("IONMODE").cloned().unwrap_or_default();
                
                let precursor_mz_str = new_spec.metadata.get("PRECURSORMZ").cloned().unwrap_or_default();
                let author_mz = precursor_mz_str.parse::<f64>().unwrap_or(0.0);
                
                if formula.is_empty() || precursor_type.is_empty() || author_mz == 0.0 {
                    return Some(new_spec);
                }
                
                let precursor_formula = adduct_mass_calculator::apply_adduct_to_formula(&formula, &precursor_type);
                let theorical_mz = adduct_mass_calculator::compute_mz(&precursor_formula).unwrap_or(0.0);
                
                if theorical_mz == 0.0 {
                    return Some(new_spec);
                }
                
                let delta = (author_mz - theorical_mz).abs();
                
                if delta <= 2.0 {
                    new_spec.metadata.insert("PRECURSOR_FORMULA".to_string(), precursor_formula);
                    new_spec.metadata.insert("EXACT_MASS_RDKit".to_string(), theorical_mz.to_string());
                    return Some(new_spec);
                }
                
                // Cherche un meilleur adduit
                let adduct_dict = if ion_mode == "positive" { &adduct_massdiff_pos } else { &adduct_massdiff_neg };
                
                let mut best_adduct: Option<String> = None;
                let mut best_delta = 1.0;
                let mut best_precursor_formula = String::new();
                let mut best_mz = 0.0;
                
                for candidate_adduct in adduct_dict.keys() {
                    let cand_formula = adduct_mass_calculator::apply_adduct_to_formula(&formula, candidate_adduct);
                    if cand_formula.is_empty() { continue; }
                    
                    if let Some(cand_mz) = adduct_mass_calculator::compute_mz(&cand_formula) {
                        let cand_delta = (author_mz - cand_mz).abs();
                        if cand_delta < best_delta {
                            best_delta = cand_delta;
                            best_adduct = Some(candidate_adduct.clone());
                            best_precursor_formula = cand_formula;
                            best_mz = cand_mz;
                        }
                    }
                }
                
                match best_adduct {
                    Some(new_adduct) => {
                        new_spec.metadata.insert("PRECURSORTYPE".to_string(), new_adduct);
                        new_spec.metadata.insert("PRECURSOR_FORMULA".to_string(), best_precursor_formula);
                        new_spec.metadata.insert("EXACT_MASS_RDKit".to_string(), best_mz.to_string());
                        Some(new_spec)
                    },
                    None => {
                        // NO_MATCH -> on supprime
                        None
                    }
                }
            }).collect();
            
            for res in processed_chunk {
                if let Some(valid_spec) = res {
                    all_processed.push(valid_spec);
                } else {
                    deletion_report.no_or_bad_adduct += 1;
                }
            }
        });
        
        total_processed += chunk.len();
        if let Some(cb) = &progress_callback {
            cb.call1(py, (total_processed,))?;
        }
    }
    
    Ok(all_processed)
}
