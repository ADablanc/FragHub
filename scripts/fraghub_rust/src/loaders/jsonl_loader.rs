// src/loaders/jsonl_loader.rs
//! Lecture des fichiers JSONL (un spectre JSON par ligne).
use pyo3::prelude::*;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::{Duration, Instant};

use super::generate_file_hash;

// ----------------------------------------------------------------------
// RUST NATIVE JSONL LOADER (Ligne par Ligne)
// ----------------------------------------------------------------------
/// RUST NATIVE JSONL LOADER (Ligne par Ligne)
///
/// # Arguments
/// * `py` (Python) : Token PyO3.
/// * `json_file_path` (&str) : Le chemin vers le fichier JSONL.
/// * `progress_callback` (Option<PyObject>) : Callback pour la progression en octets lus.
/// * `total_items_callback` (Option<PyObject>) : Callback pour la taille totale en octets.
/// * `prefix_callback` (Option<PyObject>) : Callback du préfixe.
/// * `item_type_callback` (Option<PyObject>) : Callback du type d'élément ("bytes").
///
/// # Returns
/// * `PyResult<Vec<String>>` : Une liste de spectres bruts sous forme de chaînes de caractères.
pub fn load_spectrum_list_json_2(
    py: Python,
    json_file_path: &str,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<String>> {
    let file_hash = generate_file_hash(json_file_path);
    let path = Path::new(json_file_path);
    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let total_bytes = std::fs::metadata(json_file_path).map(|m| m.len()).unwrap_or(0);
    // Progression en octets : l'interface et la ligne de commande l'affichent en MB / GB et MB/s / GB/s
    let total_size = std::cmp::max(1, total_bytes);

    if let Some(cb) = &total_items_callback { let _ = cb.call1(py, (total_size, 0)); }
    if let Some(cb) = &prefix_callback { let _ = cb.call1(py, (format!("loading [{}]:", filename),)); }
    if let Some(cb) = &item_type_callback { let _ = cb.call1(py, ("bytes",)); }

    let file = File::open(json_file_path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024 * 4, file);
    let mut spectrum_list = Vec::new();
    let mut line = String::with_capacity(65536);
    let mut total_consumed: u64 = 0;
    let mut last_update = Instant::now();

    let injection_prefix = format!(r#"{{"{}":"{}","{}":"{}","#, super::INJECTED_FILENAME, filename, super::INJECTED_FILEHASH, file_hash);

    loop {
        crate::stop::check()?; // arrêt demandé (stop.rs)
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(bytes_read) => {
                total_consumed += bytes_read as u64;
                let trimmed = line.trim();
                if !trimmed.is_empty() && trimmed.starts_with('{') {
                    let mut injected = String::with_capacity(injection_prefix.len() + trimmed.len());
                    injected.push_str(&injection_prefix);
                    injected.push_str(&trimmed[1..]);
                    spectrum_list.push(injected);

                    if last_update.elapsed() >= Duration::from_millis(50) {
                        let current_size = total_consumed;
                        if let Some(cb) = &progress_callback { let _ = cb.call1(py, (current_size,)); }
                        py.allow_threads(|| std::thread::sleep(Duration::from_millis(1)));
                        last_update = Instant::now();
                    }
                }
            }
            Err(e) => return Err(pyo3::exceptions::PyIOError::new_err(e.to_string())),
        }
    }
    
    if let Some(cb) = &progress_callback { let _ = cb.call1(py, (total_size,)); }
    
    Ok(spectrum_list)
}
