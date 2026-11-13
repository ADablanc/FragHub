use pyo3::prelude::*;
use crate::spectrum::Spectrum;
use std::collections::HashMap;
use rayon::prelude::*;
use crate::globals_vars::{INDIGO_SMILES_CORRECTION_PATTERN, INCHIKEY_PATTERN};
use std::fs;
use std::path::Path;

/// Orchestre le recalcul des formules et des masses à l'aide de RDKit (en C++, voir src/rdkit_native).
///
/// Pour un développeur Python : C'est ici qu'on fait le "pont" (bridge) entre les spectres et
/// RDKit. L'astuce cruciale est l'utilisation d'un `cache`
/// en RAM (HashMap). Comme on croise souvent les mêmes molécules, Rust va mémoriser les résultats
/// de RDKit pour ne jamais recalculer deux fois la même chose. C'est ce qui fait passer le
/// temps de calcul de plusieurs heures à quelques minutes.
pub fn process_mols(
    py: Python,
    mut spectrum_list: Vec<Spectrum>,
    output_directory: &str,
    deletion_report: &mut crate::deletion_report::DeletionReport,
    update: bool,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<Spectrum>> {

    if let Some(cb) = &prefix_callback { cb.call1(py, ("derivation and calculation (RDKit via Rust):",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("rows",))?; }

    let total = spectrum_list.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total, 0))?; }

    let mut valid_list = Vec::new();
    let mut deleted_count = 0;
    let mut deleted_rows = Vec::new();
    // Mêmes colonnes que les autres fichiers de DELETED_SPECTRUMS (DATABASE_NAME manquait)
    let columns = vec!["FILENAME", "DATABASE_NAME", "FILEHASH", "PREDICTED", "SPLASH", "SPECTRUMID", "RESOLUTION", "SYNON", "IONIZATION", "MSLEVEL", "FRAGMENTATIONMETHODE", "NAME", "PRECURSORMZ", "EXACTMASS", "AVERAGEMASS", "PRECURSORTYPE", "INSTRUMENTTYPE", "INSTRUMENT", "SMILES", "INCHI", "INCHIKEY", "COLLISIONENERGY", "FORMULA", "RT", "IONMODE", "COMMENT", "ENTROPY", "CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS", "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS", "NUM PEAKS", "PEAKS_LIST", "DELETION_REASON"];

    // 1. Extraire les molécules uniques de manière très rapide
    // Nouveau : inclut le PRECURSORTYPE et PRECURSORMZ pour la logique de fragmentation PY1b
    // Calcul en parallèle (sans garder le verrou Python) : chaque spectre donne au plus une clé,
    // calculée exactement comme avant, et l'ensemble final est le même (un ensemble n'a pas d'ordre).
    let unique_mols: std::collections::HashSet<(String, String, String)> = py.allow_threads(|| {
        spectrum_list.par_iter().filter_map(|spec| {
            let inchi = spec.metadata.get("INCHI").map(|s| s.as_str()).unwrap_or("");
            let smiles = spec.metadata.get("SMILES").map(|s| s.as_str()).unwrap_or("");
            let target_mol = if !inchi.is_empty() && inchi != "nan" { inchi } else { smiles };
            if !target_mol.is_empty() && target_mol != "nan" {
                let mut clean_mol = target_mol.to_string();
                if !clean_mol.contains("InChI=") {
                    clean_mol = INDIGO_SMILES_CORRECTION_PATTERN.replace_all(&clean_mol, "").to_string();
                }

                let precursortype = spec.metadata.get("PRECURSORTYPE").cloned().unwrap_or_default();
                let precursormz_raw = spec.metadata.get("PRECURSORMZ").map(|s| s.as_str()).unwrap_or("");
                let precursormz_str = if let Ok(mz) = precursormz_raw.parse::<f64>() {
                    format!("{:.4}", mz)
                } else {
                    "".to_string()
                };

                Some((clean_mol, precursortype, precursormz_str))
            } else {
                None
            }
        }).collect()
    });

    if let Some(cb) = &prefix_callback { cb.call1(py, ("RDKit calculation",))?; }

    // 2. Calcul RDKit : en C++ depuis Rust (src/rdkit_native).
    let total_unique = unique_mols.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_unique, 0))?; }

    let unique_mols_vec: Vec<(String, String, String)> = unique_mols.into_iter().collect();
    let mut cache = run_rdkit(py, unique_mols_vec, &progress_callback)?;

    // ==========================================
    // LOGIQUE DE RETRAITEMENT (Fallback PubChem)
    // ==========================================
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Checking failures & fallback PubChem (Pass 2)...",))?; }

    let state = crate::global_state::STATE.read().unwrap();
    let db = &state.compound_db;
    let smiles_column = db.column_id("SMILES");
    let mut unique_mols_pass2: std::collections::HashSet<(String, String, String)> = std::collections::HashSet::new();

    for spec in &mut spectrum_list {
        let inchi = spec.metadata.get("INCHI").cloned().unwrap_or_default();
        let smiles = spec.metadata.get("SMILES").cloned().unwrap_or_default();
        let used_inchi = !inchi.is_empty() && inchi != "nan";
        let mut target_mol = if used_inchi { inchi } else { smiles.clone() };

        if !target_mol.is_empty() && target_mol != "nan" {
            if !target_mol.contains("InChI=") {
                target_mol = INDIGO_SMILES_CORRECTION_PATTERN.replace_all(&target_mol, "").to_string();
            }
        }

        let precursortype = spec.metadata.get("PRECURSORTYPE").cloned().unwrap_or_default();
        let precursormz_raw = spec.metadata.get("PRECURSORMZ").map(|s| s.as_str()).unwrap_or("");
        let precursormz_str = if let Ok(mz) = precursormz_raw.parse::<f64>() {
            format!("{:.4}", mz)
        } else {
            "".to_string()
        };

        let cache_key = format!("{}|{}|{}", target_mol, precursortype, precursormz_str);

        let mut is_valid = false;
        if let Some(transforms) = cache.get(&cache_key) {
            if transforms.contains_key("EXACTMASS") {
                is_valid = true;
            }
        }

        if !is_valid {
            // RDKit a échoué. Premier secours (option 3) : le SMILES du spectre, quand c'est son InChI qui a été
            // calculée et qu'il a aussi un SMILES. Il est calculé dans la passe 2 ; s'il échoue lui aussi, le SMILES
            // de la base (ci-dessous) est pris à la place, comme avant.
            if used_inchi && !smiles.trim().is_empty() && smiles != "nan" {
                let own_smiles = INDIGO_SMILES_CORRECTION_PATTERN.replace_all(&smiles, "").to_string();
                if !own_smiles.trim().is_empty() {
                    unique_mols_pass2.insert((own_smiles.clone(), precursortype.clone(), precursormz_str.clone()));
                    spec.metadata.insert(FALLBACK_OWN_SMILES.to_string(), own_smiles);
                }
            }
            // Second secours : le SMILES de la base, trouvé par l'INCHIKEY.
            let inchikey = spec.metadata.get("INCHIKEY").cloned().unwrap_or_default();
            if INCHIKEY_PATTERN.is_match(&inchikey) {
                if let Some(loc) = db.locate(&inchikey) {
                    if let Some(fallback_smiles) = smiles_column.and_then(|id| db.value(loc, id)) {
                        if !fallback_smiles.trim().is_empty() && fallback_smiles.to_lowercase() != "nan" {
                            unique_mols_pass2.insert((fallback_smiles.clone(), precursortype, precursormz_str));
                            spec.metadata.insert("FALLBACK_SMILES_PY1".to_string(), fallback_smiles);
                        }
                    }
                }
            }
        }
    }

    if !unique_mols_pass2.is_empty() {
        if let Some(cb) = &prefix_callback { cb.call1(py, ("Processing molecules (Pass 2)...",))?; }
        let total_unique2 = unique_mols_pass2.len();
        if let Some(cb) = &total_items_callback { cb.call1(py, (total_unique2, 0))?; }
        
        let unique_mols_vec2: Vec<(String, String, String)> = unique_mols_pass2.into_iter().collect();
        let cache2 = run_rdkit(py, unique_mols_vec2, &progress_callback)?;
        
        // Fusion du second cache dans le premier
        cache.extend(cache2);
    }
    // ==========================================

    if let Some(cb) = &prefix_callback { cb.call1(py, ("applying RDKit results to all spectra...",))?; }
    if let Some(cb) = &total_items_callback { cb.call1(py, (total, 0))?; }

    // 3. Appliquer les résultats en Rust à vitesse maximale
    for (i, mut spec) in spectrum_list.into_iter().enumerate() {
        let inchi = spec.metadata.get("INCHI").cloned().unwrap_or_default();
        let smiles = spec.metadata.get("SMILES").cloned().unwrap_or_default();
        let target_mol = if !inchi.is_empty() && inchi != "nan" { inchi.clone() } else { smiles.clone() };

        let mut clean_mol = String::new();
        if !target_mol.is_empty() && target_mol != "nan" {
            clean_mol = target_mol.clone();
            if !clean_mol.contains("InChI=") {
                clean_mol = INDIGO_SMILES_CORRECTION_PATTERN.replace_all(&clean_mol, "").to_string();
            }
        }

        let precursortype = spec.metadata.get("PRECURSORTYPE").cloned().unwrap_or_default();
        let precursormz_raw = spec.metadata.get("PRECURSORMZ").map(|s| s.as_str()).unwrap_or("");
        let precursormz_str = if let Ok(mz) = precursormz_raw.parse::<f64>() {
            format!("{:.4}", mz)
        } else {
            "".to_string()
        };

        // Secours quand le calcul de la molécule du spectre a échoué (passe 2) : SMILES du spectre s'il a donné un
        // résultat, sinon SMILES de la base (qui remplace alors le SMILES du spectre dans la sortie, comme avant).
        let own = spec.metadata.remove(FALLBACK_OWN_SMILES);
        let from_db = spec.metadata.remove("FALLBACK_SMILES_PY1");
        let computed = |mol: &str| cache.get(&format!("{}|{}|{}", mol, precursortype, precursormz_str))
            .map_or(false, |t| t.contains_key("EXACTMASS"));
        match choose_fallback(own, from_db, computed) {
            Some(Fallback::OwnSmiles(s)) => clean_mol = s,
            Some(Fallback::DatabaseSmiles(s)) => {
                clean_mol = s.clone();
                spec.metadata.insert("SMILES".to_string(), s);
            }
            None => {}
        }
        
        let cache_key = format!("{}|{}|{}", clean_mol, precursortype, precursormz_str);

        if !clean_mol.is_empty() {
            if let Some(transforms) = cache.get(&cache_key) {
                for (k, v) in transforms {
                    spec.metadata.insert(k.clone(), computed_mass_text(k, v));
                }
            }
        }

        // Vérification finale
        let final_ik = spec.metadata.get("INCHIKEY").cloned().unwrap_or_default();
        let final_em = spec.metadata.get("EXACTMASS").cloned().unwrap_or_default();
        
        if INCHIKEY_PATTERN.is_match(&final_ik) && !final_em.is_empty() && final_em != "nan" {
            valid_list.push(spec);
        } else {
            deleted_count += 1;
            spec.metadata.insert("DELETION_REASON".to_string(), "spectrum deleted because it has neither inchi nor smiles nor inchikey, even after re calculation (including PubChem fallback)".to_string());
            
            let mut row_vals = Vec::new();
            for col in &columns {
                if *col == "PEAKS_LIST" {
                    row_vals.push(crate::spectrum::peaks_list_text(&spec));
                } else {
                    row_vals.push(spec.metadata.get(*col).cloned().unwrap_or_default());
                }
            }
            deleted_rows.push(row_vals);
        }

        if (i + 1) % 1000 == 0 {
            if let Some(cb) = &progress_callback { cb.call1(py, (i + 1,))?; }
        }
    }

    if let Some(cb) = &progress_callback { cb.call1(py, (total,))?; }

    deletion_report.no_smiles_no_inchi_no_inchikey += deleted_count;

    // Écriture des suppressions si besoin
    if !deleted_rows.is_empty() {
        let del_dir = Path::new(output_directory).join("DELETED_SPECTRUMS");
        fs::create_dir_all(&del_dir).unwrap_or_default();
        let file_path = del_dir.join("deleted_no_inchi_smiles_inchikey_after_re_calculation.csv");

        // On preserve l'historique des runs precedents (mode append, sans reset_updates) :
        // si le fichier existe deja, on ajoute les lignes a la suite au lieu de l'ecraser.
        let is_append = update && file_path.exists();
        // Noté avant écriture : remis comme avant si le run s'arrête (output_guard.rs)
        crate::output_guard::prepare(&file_path, is_append).unwrap();
        let file = std::fs::OpenOptions::new().write(true).create(true).append(is_append).truncate(!is_append).open(&file_path).unwrap();
        let mut wtr = csv::WriterBuilder::new().delimiter(b'\t').has_headers(!is_append).from_writer(file);
        if !is_append {
            wtr.write_record(&columns).unwrap_or_default();
        }
        for row in deleted_rows {
            wtr.write_record(&row).unwrap_or_default();
        }
        wtr.flush().unwrap_or_default();
    }

    Ok(valid_list)
}


/// Clé provisoire : SMILES du spectre à essayer quand son InChI n'a pas pu être calculée (retirée avant l'écriture).
const FALLBACK_OWN_SMILES: &str = "FALLBACK_SMILES_OWN";

/// Secours retenu pour un spectre dont la molécule n'a pas pu être calculée.
#[derive(Debug, PartialEq)]
enum Fallback {
    /// SMILES du spectre (son InChI avait échoué) : la colonne SMILES du spectre est gardée.
    OwnSmiles(String),
    /// SMILES de la base interne : il remplace le SMILES du spectre.
    DatabaseSmiles(String),
}

/// Ordre des secours : le SMILES du spectre s'il a été calculé, sinon le SMILES de la base (gardé même s'il a
/// échoué, comme avant : le spectre est alors supprimé à la vérification finale).
fn choose_fallback(own: Option<String>, from_db: Option<String>, computed: impl Fn(&str) -> bool) -> Option<Fallback> {
    match own {
        Some(s) if computed(&s) => Some(Fallback::OwnSmiles(s)),
        _ => from_db.map(Fallback::DatabaseSmiles),
    }
}

/// Calcule les molécules avec RDKit et renvoie le dictionnaire "molécule|adduit|m/z" -> résultats.
/// Calcul en C++ (src/rdkit_native), réparti sur tous les cœurs.
/// Masses calculées par RDKit (EXACTMASS, AVERAGEMASS) : écrites avec 8 décimales au plus. La somme des masses des
/// atomes donnait parfois « 893.1832885220006 » ou « 893.6960000000009 ». Les autres valeurs (InChI, SMILES,
/// formule...) et une masse illisible sont rendues telles quelles.
fn computed_mass_text(key: &str, value: &str) -> String {
    if key == "EXACTMASS" || key == "AVERAGEMASS" {
        if let Ok(mass) = value.parse::<f64>() {
            if mass.is_finite() {
                return crate::spectrum::computed_text(mass);
            }
        }
    }
    value.to_string()
}

fn run_rdkit(
    py: Python,
    mols: Vec<(String, String, String)>,
    progress_callback: &Option<PyObject>,
) -> PyResult<HashMap<String, HashMap<String, String>>> {
    crate::rdkit_native::run_parallel(py, mols, progress_callback.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secours_smiles_du_spectre_avant_la_base() {
        let ok = |m: &str| m == "CCO" || m == "OCC";
        // InChI en échec, SMILES du spectre calculé : il est pris, même si la base a un SMILES
        assert_eq!(choose_fallback(Some("CCO".into()), Some("OCC".into()), ok), Some(Fallback::OwnSmiles("CCO".into())));
        // SMILES du spectre en échec lui aussi : SMILES de la base
        assert_eq!(choose_fallback(Some("C1CC".into()), Some("OCC".into()), ok), Some(Fallback::DatabaseSmiles("OCC".into())));
        // Pas de SMILES dans le spectre : la base, comme avant l'option 3
        assert_eq!(choose_fallback(None, Some("OCC".into()), ok), Some(Fallback::DatabaseSmiles("OCC".into())));
        // Secours de la base gardé même en échec (le spectre sera supprimé à la vérification finale, comme avant)
        assert_eq!(choose_fallback(None, Some("X".into()), ok), Some(Fallback::DatabaseSmiles("X".into())));
        // Rien à essayer
        assert_eq!(choose_fallback(Some("C1CC".into()), None, ok), None);
        assert_eq!(choose_fallback(None, None, ok), None);
    }

    #[test]
    fn masses_calculees_huit_decimales_au_plus() {
        // Valeurs réelles (MoNA022203, 2-Hydroxycyclohexane-1-carboxyl-CoA)
        assert_eq!(computed_mass_text("EXACTMASS", "893.1832885220006"), "893.18328852");
        assert_eq!(computed_mass_text("AVERAGEMASS", "893.6960000000009"), "893.696");
        // Déjà courtes : inchangées
        assert_eq!(computed_mass_text("EXACTMASS", "194.08037556"), "194.08037556");
        assert_eq!(computed_mass_text("AVERAGEMASS", "194.194"), "194.194");
        // Masse illisible, et autres champs (même s'ils ressemblent à un nombre) : rendus tels quels
        assert_eq!(computed_mass_text("EXACTMASS", "nan"), "nan");
        assert_eq!(computed_mass_text("EXACTMASS", ""), "");
        assert_eq!(computed_mass_text("FORMULA", "893.1832885220006"), "893.1832885220006");
        assert_eq!(computed_mass_text("SMILES", "CCO"), "CCO");
    }
}
