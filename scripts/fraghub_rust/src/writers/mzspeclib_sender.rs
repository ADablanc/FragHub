// src/writers/mzspeclib_sender.rs
//! Sortie mzSpecLib : les spectres de chaque bibliothèque (POS_LC, NEG_LC...) sont envoyés par paquets à un
//! objet Python (`scripts/mzspeclib_converter.py`, classe `MzSpecLibSink`), qui les traduit en termes du
//! vocabulaire PSI-MS et les fait écrire par la bibliothèque `mzspeclib`, en parallèle.
//!
//! Chaque spectre est envoyé sous la forme du texte JSON d'un spectre FragHub : mêmes noms de champs et mêmes
//! valeurs que dans la sortie JSON (`json_writer::json_map`). Aucun fichier intermédiaire : seul un paquet à
//! la fois est en mémoire côté Python (plus ceux en cours de conversion, nombre limité par le Python).
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;
use std::path::Path;

use super::json_writer::json_map;

/// Nombre de spectres par paquet envoyé au Python (un paquet = une tâche d'un processus de conversion).
pub(super) const SEND_CHUNK: usize = 1000;

/// Envoie les spectres des huit bibliothèques à `sink` :
/// `sink.begin(chemin, nom, update, version de FragHub)`, puis `sink.add(liste de textes JSON)` par paquet, puis `sink.end()`.
/// Le fichier écrit est `<sortie>/mzSpecLib/<POS|NEG>/<nom>.mzSpecLib.json` (mêmes noms qu'avant).
/// `check_stop` est appelé entre deux paquets (bouton stop).
#[allow(clippy::too_many_arguments)]
pub fn sending_mzspeclib_processing(
    py: Python, update: bool, libs: &crate::splitter::Libraries, ordered_columns: &[String], output_directory: &str, sink: &PyObject, check_stop: &dyn Fn() -> PyResult<()>, progress_callback: Option<PyObject>, total_items_callback: Option<PyObject>, prefix_callback: Option<PyObject>, item_type_callback: Option<PyObject>,
) -> PyResult<()> {
    let tasks: [(&[Spectrum], &str, &str); 12] = [
        (&libs.pos_lc, "POS_LC", "POS"),
        (&libs.pos_lc_insilico, "POS_LC_insilico", "POS"),
        (&libs.pos_gc, "POS_GC", "POS"),
        (&libs.pos_gc_insilico, "POS_GC_insilico", "POS"),
        (&libs.neg_lc, "NEG_LC", "NEG"),
        (&libs.neg_lc_insilico, "NEG_LC_insilico", "NEG"),
        (&libs.neg_gc, "NEG_GC", "NEG"),
        (&libs.neg_gc_insilico, "NEG_GC_insilico", "NEG"),
        // Injection directe (DI)
        (&libs.pos_di, "POS_DI", "POS"),
        (&libs.pos_di_insilico, "POS_DI_insilico", "POS"),
        (&libs.neg_di, "NEG_DI", "NEG"),
        (&libs.neg_di_insilico, "NEG_DI_insilico", "NEG"),
    ];
    let total_items: usize = tasks.iter().map(|(list, _, _)| list.len()).sum();
    if total_items == 0 { return Ok(()); }

    if let Some(cb) = &prefix_callback { cb.call1(py, ("Writing mzSpecLib files:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items, 0))?; }

    let mut done = 0usize;
    for (data_list, name, mode) in tasks.iter() {
        if data_list.is_empty() { continue; }
        let file_path = Path::new(output_directory).join("mzSpecLib").join(mode).join(format!("{name}.mzSpecLib.json"));
        if let Some(cb) = &prefix_callback { cb.call1(py, (format!("Writing {name}.mzSpecLib.json to disk..."),))?; }
        sink.call_method1(py, "begin", (file_path.to_string_lossy().to_string(), *name, update, env!("CARGO_PKG_VERSION")))?;
        for chunk in data_list.chunks(SEND_CHUNK) {
            if let Err(e) = check_stop() {
                let _ = sink.call_method0(py, "abort");
                return Err(e);
            }
            // Mise en forme en parallèle, sans le GIL ; texte compact (le Python le relit avec json.loads)
            let texts: Vec<String> = py.allow_threads(|| {
                chunk.par_iter().map(|spec| serde_json::Value::Object(json_map(spec, ordered_columns)).to_string()).collect()
            });
            sink.call_method1(py, "add", (texts,))?;
            done += chunk.len();
            if let Some(cb) = &progress_callback { cb.call1(py, (done,))?; }
        }
        sink.call_method0(py, "end")?;
    }
    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }
    Ok(())
}
