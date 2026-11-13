// src/update_checker.rs
use pyo3::prelude::*;
use crate::spectrum::Spectrum;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use csv::WriterBuilder;

/// Vérifie quels spectres ont déjà été traités lors d'une session précédente.
///
/// Pour un développeur Python : Un "Set" (ensemble) en Python utilise des hash tables. En Rust, 
/// on utilise un `HashSet`. La différence est que la recherche `.contains()` est garantie en `O(1)`
/// de manière très stricte et déterministe au niveau de la RAM, là où Python ajoute une couche d'abstraction ("overhead").
pub fn check_for_update_processing(
    py: Python,
    spectrum_list: Vec<Spectrum>,
    output_directory: String,
    ordered_columns: Vec<String>,
    append_mode: bool,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<(Vec<Spectrum>, bool, usize)> {

    let total_items = spectrum_list.len();

    // ⚠️ ORDRE CRITIQUE POUR L'INTERFACE VUE.JS
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items,))?; }
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Checking for updates:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }

    // 1. Lire le fichier updates.json existant
    let update_file_path = Path::new(&output_directory).join("updates.json");
    let mut splash_set: HashSet<String> = HashSet::new();
    let mut json_data = serde_json::json!({"SPLASH_LIST": {}});

    if update_file_path.exists() {
        if let Ok(file) = fs::File::open(&update_file_path) {
            if let Ok(parsed) = serde_json::from_reader::<_, serde_json::Value>(file) {
                json_data = parsed;
                if let Some(splash_list) = json_data.get("SPLASH_LIST").and_then(|v| v.as_object()) {
                    for key in splash_list.keys() {
                        splash_set.insert(key.clone()); // On stocke les SPLASH connus dans un Hash ultra-rapide
                    }
                }
            }
        }
    }

    // --- CORRECTION : Utilisation d'un HashSet pour une recherche en O(1) ---
    let mut indices_to_keep: HashSet<usize> = HashSet::new();
    let mut indices_to_delete = Vec::new();
    let mut new_splashes = Vec::new();
    let mut processed = 0;

    // 2. Parcourir les spectres
    for (i, spec) in spectrum_list.iter().enumerate() {
        crate::stop::check()?; // arrêt demandé (stop.rs)
        let splash = spec.metadata.get("SPLASH").cloned().unwrap_or_default();

        if !splash.is_empty() && splash_set.contains(&splash) {
            indices_to_delete.push(i); // Déjà vu -> on supprime
        } else {
            // --- CORRECTION : Utilisation de .insert() au lieu de .push() ---
            indices_to_keep.insert(i); // Nouveau -> on garde
            if !splash.is_empty() {
                new_splashes.push(splash);
            }
        }

        processed += 1;
        if processed % 1000 == 0 {
            if let Some(cb) = &progress_callback { cb.call1(py, (processed,))?; }
        }
    }

    // ⚠️ GARANTIE DU 100% DE L'ÉTAPE 2
    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Saving updates and logs...",))?; }

    let (final_list, update, deleted_count) = py.allow_threads(|| {
        // 3. Écrire les doublons supprimés dans le CSV
        if !indices_to_delete.is_empty() {
            let deleted_dir = Path::new(&output_directory).join("DELETED_SPECTRUMS");
            fs::create_dir_all(&deleted_dir)?;
            let file_path = deleted_dir.join("previously_cleaned.csv");

            // On preserve l'historique des runs precedents (mode append, sans reset_updates) :
            // si le fichier existe deja, on ajoute les lignes a la suite au lieu de l'ecraser.
            let is_append = append_mode && file_path.exists();
            // Noté avant écriture : remis comme avant si le run s'arrête (output_guard.rs)
            crate::output_guard::prepare(&file_path, is_append).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            let file = std::fs::OpenOptions::new().write(true).create(true).append(is_append).truncate(!is_append).open(&file_path)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

            let mut wtr = WriterBuilder::new()
                .delimiter(b'\t')
                .quote(b'"')
                .has_headers(!is_append)
                .from_writer(file);

            if !is_append {
                let mut header = ordered_columns.clone();
                header.push("DELETION_REASON".to_string());
                wtr.write_record(&header).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            }

            for &idx in &indices_to_delete {
                let spec = &spectrum_list[idx];
                let mut record: Vec<String> = Vec::with_capacity(ordered_columns.len() + 1);

                for col in &ordered_columns {
                    record.push(spec.metadata.get(col).cloned().unwrap_or_default());
                }
                record.push("spectrum deleted because already processed in a previous run.".to_string());
                wtr.write_record(&record).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            }
            wtr.flush()?;
        }

        // 4. Mettre à jour le fichier JSON
        let update = !new_splashes.is_empty();
        if update {
            crate::output_guard::prepare(&update_file_path, false).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            let file = std::fs::File::create(&update_file_path).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            let mut wtr = std::io::BufWriter::new(file);
            use std::io::Write;
            
            wtr.write_all(b"{\"SPLASH_LIST\":{").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            
            let mut first = true;
            for s in &splash_set {
                if !first { wtr.write_all(b",").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?; }
                serde_json::to_writer(&mut wtr, s).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
                wtr.write_all(b":true").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
                first = false;
            }
            // Un même SPLASH peut appartenir à plusieurs spectres du run (doublons retirés plus tard,
            // ou même spectre avec une autre InChIKey) : on ne l'écrit qu'une fois.
            let mut written: HashSet<&String> = HashSet::new();
            for s in &new_splashes {
                if !written.insert(s) { continue; }
                if !first { wtr.write_all(b",").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?; }
                serde_json::to_writer(&mut wtr, s).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
                wtr.write_all(b":true").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
                first = false;
            }
            
            wtr.write_all(b"}}").map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            wtr.flush().map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        }

        // 5. Générer la liste finale
        let mut final_list = Vec::with_capacity(indices_to_keep.len());
        let mut current_idx = 0;
        for spec in spectrum_list.into_iter() {
            if indices_to_keep.contains(&current_idx) {
                final_list.push(spec);
            }
            current_idx += 1;
        }

        Ok::<_, pyo3::PyErr>((final_list, update, indices_to_delete.len()))
    })?;

    // On renvoie la nouvelle liste, le booléen (y a-t-il eu une MAJ ?), et le nombre de supprimés
    Ok((final_list, update, deleted_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::test_support::with_py;

    fn spectre(splash: &str, name: &str) -> Spectrum {
        let mut s = Spectrum::default();
        s.metadata.insert("SPLASH".into(), splash.into());
        s.metadata.insert("NAME".into(), name.into());
        s
    }

    fn noms(list: &[Spectrum]) -> Vec<String> {
        list.iter().map(|s| s.metadata["NAME"].clone()).collect()
    }

    #[test]
    fn spectres_deja_traites_retires_et_nouveaux_notes() {
        let d = std::env::temp_dir().join(format!("fraghub_updates_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("updates.json"), r#"{"SPLASH_LIST":{"splash-ancien":true}}"#).unwrap();
        let cols = vec!["NAME".to_string(), "SPLASH".to_string()];
        let list = vec![spectre("splash-ancien", "A"), spectre("splash-nouveau", "B"), spectre("splash-nouveau", "C"), spectre("", "D")];
        let (kept, updated, deleted) = with_py(|py| check_for_update_processing(
            py, list, d.to_string_lossy().to_string(), cols.clone(), false, None, None, None, None)).unwrap();
        assert_eq!(noms(&kept), vec!["B", "C", "D"]);   // ordre gardé ; sans SPLASH : gardé
        assert!(updated);
        assert_eq!(deleted, 1);
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(d.join("updates.json")).unwrap()).unwrap();
        let splashes = json["SPLASH_LIST"].as_object().unwrap();
        assert_eq!(splashes.len(), 2);   // nouveau SPLASH écrit une seule fois
        assert!(splashes.contains_key("splash-ancien") && splashes.contains_key("splash-nouveau"));
        let csv_text = fs::read_to_string(d.join("DELETED_SPECTRUMS").join("previously_cleaned.csv")).unwrap();
        let lines: Vec<&str> = csv_text.lines().collect();
        assert_eq!(lines[0], "NAME\tSPLASH\tDELETION_REASON");
        assert_eq!(lines[1], "A\tsplash-ancien\tspectrum deleted because already processed in a previous run.");

        // Run suivant en mode ajout : B et C sont maintenant connus, la ligne est ajoutée sans nouvel en-tête
        let (kept, updated, deleted) = with_py(|py| check_for_update_processing(
            py, vec![spectre("splash-nouveau", "B2")], d.to_string_lossy().to_string(), cols, true, None, None, None, None)).unwrap();
        assert!(kept.is_empty() && !updated && deleted == 1);
        let csv_text = fs::read_to_string(d.join("DELETED_SPECTRUMS").join("previously_cleaned.csv")).unwrap();
        assert_eq!(csv_text.lines().count(), 3);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn premier_run_sans_fichier_de_mises_a_jour() {
        let d = std::env::temp_dir().join(format!("fraghub_updates_vide_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let (kept, updated, deleted) = with_py(|py| check_for_update_processing(
            py, vec![spectre("s1", "A")], d.to_string_lossy().to_string(), vec!["NAME".into()], false, None, None, None, None)).unwrap();
        assert_eq!((kept.len(), updated, deleted), (1, true, 0));
        assert!(!d.join("DELETED_SPECTRUMS").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
