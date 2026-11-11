// src/writers/msp_writer.rs
//! Écriture des fichiers MSP : le texte de chaque spectre (spectra_to_msp::msp_text) est fabriqué EN PARALLÈLE par
//! paquets et écrit DANS L'ORDRE, comme pour le CSV et le JSON. Avant, le texte de tous les spectres était préparé
//! d'un coup puis gardé en mémoire jusqu'à la fin de l'écriture (plusieurs Go sur une base complète) ; les fichiers
//! obtenus sont identiques.
use pyo3::prelude::*;
use crate::spectrum::Spectrum;
use crate::spectra_to_msp::msp_text;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::{WRITE_CHUNK, WRITE_CHUNKS_PER_ROUND};

/// Écrit les fichiers MSP (MSP/POS, MSP/NEG), un par table de spectres.
///
/// # Arguments
/// * `py` (Python) : Token d'accès au GIL.
/// * `libs` (&Libraries) : les 12 bibliothèques (POS/NEG × LC/GC/DI × expérimental/in silico).
/// * `output_directory` (&str) : Dossier cible.
/// * `update` (bool) : Indicateur pour savoir s'il faut écraser ou ajouter (append) au fichier existant.
/// * `progress_callback`, `total_items_callback`, `prefix_callback`, `item_type_callback` : Callbacks UI.
///
/// # Returns
/// * `PyResult<()>` : Succès ou erreur IO.
#[allow(clippy::too_many_arguments)]
pub fn writting_msp_processing(
    py: Python, libs: &crate::splitter::Libraries, output_directory: &str, update: bool, progress_callback: Option<PyObject>, total_items_callback: Option<PyObject>, prefix_callback: Option<PyObject>, item_type_callback: Option<PyObject>,
) -> PyResult<()> {

    let tasks: Vec<(&[Spectrum], &str, &str)> = vec![
        (&libs.pos_lc, "POS_LC.msp", "POS"),
        (&libs.pos_lc_insilico, "POS_LC_insilico.msp", "POS"),
        (&libs.pos_gc, "POS_GC.msp", "POS"),
        (&libs.pos_gc_insilico, "POS_GC_insilico.msp", "POS"),
        (&libs.neg_lc, "NEG_LC.msp", "NEG"),
        (&libs.neg_lc_insilico, "NEG_LC_insilico.msp", "NEG"),
        (&libs.neg_gc, "NEG_GC.msp", "NEG"),
        (&libs.neg_gc_insilico, "NEG_GC_insilico.msp", "NEG"),
        // Injection directe (DI)
        (&libs.pos_di, "POS_DI.msp", "POS"),
        (&libs.pos_di_insilico, "POS_DI_insilico.msp", "POS"),
        (&libs.neg_di, "NEG_DI.msp", "NEG"),
        (&libs.neg_di_insilico, "NEG_DI_insilico.msp", "NEG"),
    ];

    let total_items: usize = tasks.iter().map(|(list, _, _)| list.len()).sum();
    if total_items == 0 { return Ok(()); }

    if let Some(cb) = &prefix_callback { cb.call1(py, ("Writing MSP files:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items, 0))?; }

    let global_processed = Arc::new(AtomicUsize::new(0));
    let progress_cb = progress_callback.clone();

    let results: Vec<Result<(), String>> = py.allow_threads(|| {
        tasks.par_iter().map(|(data_list, filename, mode)| {
            if data_list.is_empty() { return Ok(()); }

            let path_dir = Path::new(output_directory).join("MSP").join(mode);
            fs::create_dir_all(&path_dir).map_err(|e| e.to_string())?;
            let file_path = path_dir.join(filename);

            write_msp_file(data_list, &file_path, update, &|n| {
                let current = global_processed.fetch_add(n, Ordering::Relaxed) + n;
                if let Some(ref cb) = progress_cb {
                    Python::with_gil(|py| { let _ = cb.call1(py, (current,)); });
                }
            })
        }).collect()
    });

    for res in results {
        if let Err(e) = res { return Err(crate::stop::io_error(e)); }
    }

    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }
    Ok(())
}

/// Écrit (ou complète en mode mise à jour) un fichier MSP : chaque spectre, suivi d'une ligne vide.
/// `progress(n)` après n spectres écrits.
pub(crate) fn write_msp_file(
    data_list: &[Spectrum],
    file_path: &Path,
    update: bool,
    progress: &(dyn Fn(usize) + Sync),
) -> Result<(), String> {
    // Noté avant écriture : remis comme avant si le run s'arrête (output_guard.rs)
    crate::output_guard::prepare(file_path, update && file_path.exists()).map_err(|e| e.to_string())?;
    let file = OpenOptions::new().write(true).create(true).append(update).truncate(!update).open(file_path).map_err(|e| e.to_string())?;
    let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
    let round_size = WRITE_CHUNK * WRITE_CHUNKS_PER_ROUND;
    for round in data_list.chunks(round_size) {
        crate::stop::check_str()?; // arrêt demandé (stop.rs)
        let buffers: Vec<Vec<u8>> = round.par_chunks(WRITE_CHUNK).map(|chunk| {
            let mut out = Vec::with_capacity(chunk.len() * 4096);
            for spec in chunk {
                out.extend_from_slice(msp_text(spec).as_bytes());
                out.extend_from_slice(b"\n\n");
            }
            out
        }).collect();
        for buf in buffers {
            file.write_all(&buf).map_err(|e| e.to_string())?;
        }
        progress(round.len());
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_data::{jeu, dossier};

    // Ancienne écriture : textes de tous les spectres préparés d'abord, puis écrits un par un.
    fn ancien_msp(data_list: &[Spectrum], file_path: &Path, update: bool) {
        let texts: Vec<String> = data_list.iter().map(msp_text).collect();
        let mut file = OpenOptions::new().write(true).create(true).append(update).truncate(!update).open(file_path).unwrap();
        for spec_str in &texts {
            file.write_all(spec_str.as_bytes()).unwrap();
            file.write_all(b"\n\n").unwrap();
        }
        file.flush().unwrap();
    }

    #[test]
    fn msp_identique_a_l_ancienne_ecriture() {
        let d = dossier("msp");
        for &n in &[1usize, 2, 1999, 2000, 2001, 2000 * 64 + 3] {
            let (_cols, specs) = jeu(n, 11 + n as u64);
            let (a, b) = (d.join(format!("a{n}.msp")), d.join(format!("b{n}.msp")));
            ancien_msp(&specs, &a, false);
            write_msp_file(&specs, &b, false, &|_| {}).unwrap();
            // mode mise à jour : ajout à la suite
            ancien_msp(&specs[..(n / 2).max(1)], &a, true);
            write_msp_file(&specs[..(n / 2).max(1)], &b, true, &|_| {}).unwrap();
            assert!(fs::read(&a).unwrap() == fs::read(&b).unwrap(), "MSP différent pour n = {n}");
        }
        let _ = fs::remove_dir_all(&d);
    }
}
