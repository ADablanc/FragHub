// src/duplicatas_remover.rs
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use csv::WriterBuilder;

/// Colonnes jamais complétées depuis un doublon : elles décrivent le spectre gardé lui-même (fichier, base,
/// identifiant, empreinte, statut in silico, commentaire) ou sont recalculées plus loin (entropie, pics).
const NOT_FILLED_FROM_DUPLICATES: [&str; 11] = ["FILENAME", "DATABASE_NAME", "FILEHASH", "SPECTRUMID", "SPLASH",
    "INCHIKEY", "PREDICTED", "COMMENT", "ENTROPY", "NUM PEAKS", "PEAKS_LIST"];

fn is_blank(v: Option<&String>) -> bool {
    v.map_or(true, |s| { let t = s.trim(); t.is_empty() || t.eq_ignore_ascii_case("nan") })
}

/// Valeurs à copier dans le spectre gardé depuis ses doublons supprimés (mêmes pics, même molécule) :
/// seulement pour ses colonnes vides, avec la première valeur non vide des doublons (dans l'ordre de lecture).
/// Aucune valeur existante n'est remplacée. Avant, le doublon gardé (celui qui avait le plus de caractères de
/// métadonnées) pouvait ne pas avoir le temps de rétention ou l'énergie de collision que l'autre avait
/// (Metaboanalyst NEG : 283 RT et 61 énergies perdus sur 2 744 doublons).
fn values_from_duplicates(kept: &HashMap<String, String>, removed: &[&HashMap<String, String>], columns: &[String]) -> Vec<(String, String)> {
    let mut fills = Vec::new();
    for col in columns {
        if NOT_FILLED_FROM_DUPLICATES.contains(&col.as_str()) || !is_blank(kept.get(col)) {
            continue;
        }
        if let Some(v) = removed.iter().filter_map(|m| m.get(col)).find(|v| !is_blank(Some(v))) {
            fills.push((col.clone(), v.trim().to_string()));
        }
    }
    fills
}

/// Supprime les doublons (spectres identiques) en gardant celui qui contient le plus de métadonnées.
/// Les colonnes vides du spectre gardé sont complétées avec les valeurs de ses doublons (`values_from_duplicates`).
///
/// Pour un développeur Python : Retirer des doublons est très long si c'est mal optimisé (`drop_duplicates` etc.).
/// En Rust, on fait un seul passage (`O(N)`) sur les spectres. On calcule la taille (en caractères)
/// des métadonnées de chaque spectre, et on stocke l'index du "gagnant" (le plus gros) dans
/// une table de hachage (`best_indices`) utilisant un Tuple `(SPLASH, INCHIKEY)` comme clé composite.
pub fn remove_duplicatas_processing(
    py: Python,
    spectrum_list: Vec<Spectrum>,
    output_directory: String,
    ordered_columns: Vec<String>,
    update: bool,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<(Vec<Spectrum>, usize)> {

    let total_items = spectrum_list.len();

    // ⚠️ ORDRE CRITIQUE POUR L'INTERFACE VUE.JS
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items,))?; }
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Removing duplicates:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }

    let mut best_indices: HashMap<(String, String), (usize, usize)> = HashMap::new();
    // Bases d'origine de chaque groupe de doublons (pour le rapport UpSet : sinon, seule la base du
    // spectre gardé restait connue, et le rapport ne pouvait montrer aucun recoupement entre bases)
    let mut group_dbs: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    let mut empty_inchi_indices: Vec<usize> = Vec::new();
    // Groupe (SPLASH, INCHIKEY) de chaque spectre, pour compléter le gagnant avec ses doublons
    let mut group_of: Vec<Option<(String, String)>> = vec![None; total_items];

    let mut processed = 0;

    // Taille des métadonnées de chaque spectre, calculée d'abord en parallèle (même valeur qu'avant ; le texte des
    // pics, qui n'est plus gardé en mémoire, est refabriqué pour être compté, ce qui rendait la boucle plus lente)
    let row_sizes: Vec<usize> = py.allow_threads(|| {
        spectrum_list.par_iter().map(|spec| {
            let mut row_size = 0;
            for col in &ordered_columns {
                if col == "PEAKS_LIST" {
                    // Le texte des pics compte comme avant, quand il était gardé dans les métadonnées
                    row_size += match spec.metadata.get(col) {
                        Some(val) => val.chars().count(),
                        None => crate::spectrum::peaks_text(&spec.peaks).len(),
                    };
                } else if let Some(val) = spec.metadata.get(col) {
                    row_size += val.chars().count();
                }
            }
            row_size
        }).collect()
    });

    for (i, spec) in spectrum_list.iter().enumerate() {
        crate::stop::check()?; // arrêt demandé (stop.rs)
        let row_size = row_sizes[i];

        let splash = spec.metadata.get("SPLASH").cloned().unwrap_or_default();
        let inchikey = spec.metadata.get("INCHIKEY").map(|s| s.trim().to_string()).unwrap_or_default();

        if inchikey.is_empty() || inchikey.to_lowercase() == "nan" || inchikey.to_lowercase() == "none" {
            empty_inchi_indices.push(i);
        } else {
            let key = (splash, inchikey);
            group_of[i] = Some(key.clone());
            let db = spec.metadata.get("DATABASE_NAME").map(|s| s.trim().to_string()).unwrap_or_default();
            group_dbs.entry(key.clone()).or_default().insert(db);
            if let Some(&(_, best_size)) = best_indices.get(&key) {
                if row_size > best_size {
                    best_indices.insert(key, (i, row_size));
                }
            } else {
                best_indices.insert(key, (i, row_size));
            }
        }

        processed += 1;
        if processed % 1000 == 0 {
            if let Some(cb) = &progress_callback { cb.call1(py, (processed,))?; }
        }
    }

    let mut indices_to_keep: HashSet<usize> = HashSet::new();
    for &idx in &empty_inchi_indices { indices_to_keep.insert(idx); }
    for &(idx, _) in best_indices.values() { indices_to_keep.insert(idx); }

    let mut indices_to_delete: Vec<usize> = Vec::new();
    for i in 0..total_items {
        if !indices_to_keep.contains(&i) {
            indices_to_delete.push(i);
        }
    }

    if !indices_to_delete.is_empty() {
        let deleted_dir = Path::new(&output_directory).join("DELETED_SPECTRUMS");
        fs::create_dir_all(&deleted_dir)?;
        let file_path = deleted_dir.join("duplicatas_removed.csv");

        // On preserve l'historique des runs precedents (mode append, sans reset_updates) :
        // si le fichier existe deja, on ajoute les lignes a la suite au lieu de l'ecraser.
        let is_append = update && file_path.exists();
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
                if col == "PEAKS_LIST" {
                    record.push(crate::spectrum::peaks_list_text(spec));
                } else {
                    record.push(spec.metadata.get(col).cloned().unwrap_or_default());
                }
            }
            record.push("spectrum deleted because it's a duplicate (SPLASH + INCHIKEY)".to_string());
            wtr.write_record(&record).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        }
        wtr.flush()?;
    }

    // Spectre gardé dont les doublons venaient de plusieurs bases : liste interne de ces bases
    // (clé jamais écrite dans les sorties ; DATABASE_NAME reste celle du spectre gardé)
    let mut all_dbs: HashMap<usize, String> = HashMap::new();
    for (key, &(idx, _)) in &best_indices {
        if let Some(dbs) = group_dbs.get(key) {
            if dbs.len() > 1 {
                let list: Vec<&str> = dbs.iter().map(|s| s.as_str()).collect();
                all_dbs.insert(idx, list.join(crate::globals_vars::INTERNAL_LIST_SEP));
            }
        }
    }

    // Valeurs reprises des doublons pour les colonnes vides de chaque gagnant
    let mut removed_by_group: HashMap<&(String, String), Vec<&HashMap<String, String>>> = HashMap::new();
    for &idx in &indices_to_delete {
        if let Some(key) = &group_of[idx] {
            removed_by_group.entry(key).or_default().push(&spectrum_list[idx].metadata);
        }
    }
    let mut fills_by_winner: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    for (key, removed) in &removed_by_group {
        if let Some(&(winner, _)) = best_indices.get(*key) {
            let fills = values_from_duplicates(&spectrum_list[winner].metadata, removed, &ordered_columns);
            if !fills.is_empty() {
                fills_by_winner.insert(winner, fills);
            }
        }
    }
    drop(removed_by_group);

    let mut final_list = Vec::with_capacity(indices_to_keep.len());
    let mut current_idx = 0;
    for mut spec in spectrum_list.into_iter() {
        if indices_to_keep.contains(&current_idx) {
            if let Some(fills) = fills_by_winner.remove(&current_idx) {
                for (col, val) in fills {
                    spec.metadata.insert(col, val);
                }
            }
            if let Some(list) = all_dbs.remove(&current_idx) {
                spec.metadata.insert(crate::globals_vars::ALL_DATABASE_NAMES_KEY.to_string(), list);
            }
            final_list.push(spec);
        }
        current_idx += 1;
    }

    // ⚠️ GARANTIE DU 100% POUR CLÔTURER L'INTERFACE PROPREMENT
    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }

    Ok((final_list, indices_to_delete.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn colonnes_vides_completees_par_les_doublons() {
        // Cas Metaboanalyst NEG : le doublon gardé n'a pas le RT ni l'énergie, l'autre les a
        let cols: Vec<String> = ["SPECTRUMID", "COMMENT", "RT", "COLLISIONENERGY", "INSTRUMENTTYPE", "NAME"].iter().map(|s| s.to_string()).collect();
        let kept = meta(&[("SPECTRUMID", "RIKENPlaSMA004441"), ("RT", ""), ("COLLISIONENERGY", "nan"), ("INSTRUMENTTYPE", "LC-ESI-QTOF"), ("NAME", "Rutin")]);
        let a = meta(&[("SPECTRUMID", "PR1"), ("COMMENT", "autre"), ("RT", "  "), ("COLLISIONENERGY", "45"), ("INSTRUMENTTYPE", "ESI-QFT")]);
        let b = meta(&[("SPECTRUMID", "PR2"), ("RT", "4.518883"), ("COLLISIONENERGY", "20")]);
        let mut fills = values_from_duplicates(&kept, &[&a, &b], &cols);
        fills.sort();
        // RT : première valeur non vide (b) ; énergie : premier doublon (a) ; instrument et nom gardés ;
        // identifiant et commentaire jamais repris
        assert_eq!(fills, vec![("COLLISIONENERGY".to_string(), "45".to_string()), ("RT".to_string(), "4.518883".to_string())]);
        // Rien à compléter
        assert!(values_from_duplicates(&kept, &[], &cols).is_empty());
    }

    fn spectrum(pairs: &[(&str, &str)]) -> Spectrum {
        let mut s = Spectrum::default();
        s.metadata = meta(pairs);
        s.peaks = vec![(100.5, 1.0)];
        s
    }

    #[test]
    fn un_seul_spectre_garde_par_splash_et_inchikey() {
        let dir = std::env::temp_dir().join(format!("fraghub_doublons_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cols: Vec<String> = ["DATABASE_NAME", "SPLASH", "SPECTRUMID", "NAME", "INCHIKEY", "RT", "PEAKS_LIST"].iter().map(|s| s.to_string()).collect();
        let key = "RYYVLZVUVIJVGH-UHFFFAOYSA-N";
        let list = vec![
            spectrum(&[("SPECTRUMID", "COURT"), ("SPLASH", "s1"), ("INCHIKEY", key), ("DATABASE_NAME", "MoNA"), ("NAME", "Caf"), ("RT", "2.5")]),
            spectrum(&[("SPECTRUMID", "LONG"), ("SPLASH", "s1"), ("INCHIKEY", key), ("DATABASE_NAME", "GNPS"), ("NAME", "Caffeine, nom le plus long")]),
            // Même SPLASH, autre molécule : pas un doublon
            spectrum(&[("SPECTRUMID", "AUTRE_MOLECULE"), ("SPLASH", "s1"), ("INCHIKEY", "REFJWTPEDVJJIY-UHFFFAOYSA-N")]),
            // Même molécule, autre SPLASH : pas un doublon
            spectrum(&[("SPECTRUMID", "AUTRES_PICS"), ("SPLASH", "s2"), ("INCHIKEY", key)]),
            // Sans InChIKey : jamais comparés, tous gardés
            spectrum(&[("SPECTRUMID", "SANS_CLE_1"), ("SPLASH", "s3"), ("INCHIKEY", "")]),
            spectrum(&[("SPECTRUMID", "SANS_CLE_2"), ("SPLASH", "s3"), ("INCHIKEY", "nan")]),
        ];
        let (kept, removed) = crate::loaders::test_support::with_py(|py| remove_duplicatas_processing(
            py, list, dir.to_string_lossy().to_string(), cols, false, None, None, None, None)).unwrap();

        assert_eq!(removed, 1);
        let ids: Vec<&str> = kept.iter().map(|s| s.metadata["SPECTRUMID"].as_str()).collect();
        assert_eq!(ids, ["LONG", "AUTRE_MOLECULE", "AUTRES_PICS", "SANS_CLE_1", "SANS_CLE_2"]);   // ordre de lecture gardé
        // Le plus complet est gardé ; son champ vide (RT) est repris du doublon ; ses propres valeurs restent
        let winner = &kept[0].metadata;
        assert_eq!((winner["RT"].as_str(), winner["NAME"].as_str(), winner["DATABASE_NAME"].as_str()), ("2.5", "Caffeine, nom le plus long", "GNPS"));
        // Bases d'origine du groupe, gardées en interne pour le rapport (ordre alphabétique)
        assert_eq!(winner[crate::globals_vars::ALL_DATABASE_NAMES_KEY], format!("GNPS{}MoNA", crate::globals_vars::INTERNAL_LIST_SEP));
        assert!(!kept[1].metadata.contains_key(crate::globals_vars::ALL_DATABASE_NAMES_KEY));

        // Le doublon supprimé est écrit avec son motif
        let text = std::fs::read_to_string(dir.join("DELETED_SPECTRUMS").join("duplicatas_removed.csv")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "DATABASE_NAME\tSPLASH\tSPECTRUMID\tNAME\tINCHIKEY\tRT\tPEAKS_LIST\tDELETION_REASON");
        assert!(lines[1].starts_with("MoNA\ts1\tCOURT\tCaf\t") && lines[1].ends_with("spectrum deleted because it's a duplicate (SPLASH + INCHIKEY)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sans_doublon_rien_n_est_ecrit() {
        let dir = std::env::temp_dir().join(format!("fraghub_sans_doublon_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let list = vec![spectrum(&[("SPECTRUMID", "A"), ("SPLASH", "s1"), ("INCHIKEY", "K1")]),
                        spectrum(&[("SPECTRUMID", "B"), ("SPLASH", "s2"), ("INCHIKEY", "K1")])];
        let (kept, removed) = crate::loaders::test_support::with_py(|py| remove_duplicatas_processing(
            py, list, dir.to_string_lossy().to_string(), vec!["SPECTRUMID".to_string()], false, None, None, None, None)).unwrap();
        assert_eq!((kept.len(), removed), (2, 0));
        assert!(!dir.exists());
    }
}
