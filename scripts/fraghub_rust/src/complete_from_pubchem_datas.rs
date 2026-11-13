// src/complete_from_pubchem_datas.rs
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;

/// Complète les spectres avec les données issues de PubChem.
///
/// Pour un développeur Python : Au lieu de faire un `.merge()` extrêmement coûteux
/// en RAM avec Pandas, on itère simplement sur notre liste de spectres (`iter_mut()`).
/// Si l'INCHIKEY du spectre existe dans la base interne `compound_db` (déjà chargée en RAM
/// par `loading_db.rs`), on complète le spectre sans recréer de nouvelle table (voir `fill_from_db`) :
/// - NAME : toujours le nom de la base (nom de référence, le même pour tous les spectres d'une molécule) ;
/// - INCHI, SMILES, FORMULA, EXACTMASS, AVERAGEMASS : seulement si le champ du spectre est vide. RDKit les a
///   calculés juste avant à partir de la structure du spectre, cohérente avec sa clé ; avant, la base les
///   écrasait, et une ligne fausse de la base (MoNA CoA : INCHI = « InChI=1 ») passait à tous les spectres.
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

    // --- Step 2: Récupération de la base interne (PubChem + ontologies) depuis Rust Global State ---
    let state = crate::global_state::STATE.read().unwrap();
    let db = &state.compound_db;

    let columns_to_update = ["INCHI", "SMILES", "FORMULA", "NAME", "EXACTMASS", "AVERAGEMASS"];
    // Numéros de colonnes calculés une fois pour toutes (None si la colonne n'existe pas dans la base)
    let column_ids: Vec<(&str, Option<usize>)> = columns_to_update.iter().map(|c| (*c, db.column_id(c))).collect();
    let mut processed = 0;
    let chunk_size = 500;

    // --- Step 3: Boucle de mise à jour (Multithreaded) ---
    for chunk in spectrum_list.chunks_mut(chunk_size) {
        crate::stop::check()?; // arrêt demandé (stop.rs)
        py.allow_threads(|| {
            chunk.par_iter_mut().for_each(|spec| {
                let inchikey = spec.metadata.get("INCHIKEY").cloned().unwrap_or_default();

                if !inchikey.is_empty() && inchikey.to_lowercase() != "nan" {
                    if let Some(loc) = db.locate(&inchikey) {
                        for (col, column_id) in &column_ids {
                            if let Some(new_val) = column_id.and_then(|id| db.value(loc, id)) {
                                fill_from_db(&mut spec.metadata, col, new_val);
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

/// Colonne prise dans la base même si le spectre a déjà une valeur.
const ALWAYS_FROM_DB: &[&str] = &["NAME"];

/// Recopie une valeur de la base dans le spectre : toujours pour NAME, sinon seulement si le champ du spectre
/// est vide (absent, vide ou « vide » écrit : nan, N/A… selon EMPTY_PATTERN). Une valeur vide de la base n'est
/// jamais recopiée.
fn fill_from_db(metadata: &mut std::collections::HashMap<String, String>, col: &str, db_value: String) {
    if db_value.trim().is_empty() || db_value.to_lowercase() == "nan" {
        return;
    }
    let spectrum_empty = metadata.get(col)
        .map_or(true, |v| crate::globals_vars::EMPTY_PATTERN.is_match(v.trim()));
    if spectrum_empty || ALWAYS_FROM_DB.contains(&col) {
        metadata.insert(col.to_string(), db_value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn nom_de_la_base_le_reste_seulement_si_vide() {
        let mut m: HashMap<String, String> = HashMap::new();
        m.insert("NAME".into(), "octanoyl-CoA; (Acyl-CoA); [M+H]+;".into());
        m.insert("INCHI".into(), "InChI=1S/C29H50N7O17P3S/t18-/m1/s1".into());
        m.insert("SMILES".into(), "CCCCCCCC(=O)SCC".into());
        m.insert("FORMULA".into(), "nan".into());
        // Nom : celui de la base remplace celui du spectre
        fill_from_db(&mut m, "NAME", "Octanoyl-CoA".into());
        assert_eq!(m["NAME"], "Octanoyl-CoA");
        // Structure du spectre gardée, même si la base en a une autre (ligne fausse « InChI=1 »)
        fill_from_db(&mut m, "INCHI", "InChI=1".into());
        fill_from_db(&mut m, "SMILES", "O=C(SCC)CCCCCCC".into());
        assert_eq!(m["INCHI"], "InChI=1S/C29H50N7O17P3S/t18-/m1/s1");
        assert_eq!(m["SMILES"], "CCCCCCCC(=O)SCC");
        // Champ vide (« nan ») ou absent : rempli par la base
        fill_from_db(&mut m, "FORMULA", "C29H50N7O17P3S".into());
        fill_from_db(&mut m, "EXACTMASS", "893.21967".into());
        assert_eq!(m["FORMULA"], "C29H50N7O17P3S");
        assert_eq!(m["EXACTMASS"], "893.21967");
        // Valeur vide de la base : rien n'est changé, même pour NAME
        fill_from_db(&mut m, "NAME", "".into());
        fill_from_db(&mut m, "AVERAGEMASS", "nan".into());
        assert_eq!(m["NAME"], "Octanoyl-CoA");
        assert!(!m.contains_key("AVERAGEMASS"));
    }
}
