// src/set_projects.rs
use pyo3::prelude::*;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Fonction utilitaire équivalente à `shutil.rmtree` en Python.
///
/// Pour un développeur Python : Vide le contenu du dossier récursivement.
/// C'est beaucoup plus rapide qu'en Python car on fait appel directement aux API bas niveau de l'OS.
fn remove_files_rust(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Rust gère la suppression récursive de dossiers nativement !
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Remise à zéro des mises à jour, pendant un run : le contenu du dossier de sortie est mis de côté (et non
/// supprimé), puis supprimé en fin de run réussi, ou remis si le run s'arrête (output_guard.rs).
pub fn reset_updates_for_run(output_directory: &str) -> PyResult<()> {
    crate::output_guard::set_aside_output_directory(Path::new(output_directory))?;
    Ok(())
}

#[pyfunction]
pub fn reset_updates(output_directory: String) -> PyResult<()> {
    let out_path = Path::new(&output_directory);
    let json_update_path = out_path.join("updates.json");

    // Supprime le fichier json si existant
    if json_update_path.exists() {
        let _ = fs::remove_file(&json_update_path);
    }

    // Vide le dossier
    if out_path.exists() {
        let _ = remove_files_rust(out_path);
    }

    Ok(())
}

#[pyfunction]
pub fn init_project(output_directory: String) -> PyResult<()> {
    let out_path = Path::new(&output_directory);

    // 1. Créer le dossier racine s'il n'existe pas
    if !out_path.exists() {
        fs::create_dir_all(out_path)?;
    }

    // 2. Créer updates.json (avec un objet vide "{}")
    let updates_file_path = out_path.join("updates.json");
    if !updates_file_path.exists() {
        // Noté : retiré si le run s'arrête (output_guard.rs)
        crate::output_guard::prepare(&updates_file_path, false)?;
        if let Ok(mut file) = File::create(&updates_file_path) {
            let _ = file.write_all(b"{}");
        }
    }

    // 4. Créer l'arborescence (CSV, JSON, MSP) x (NEG, POS)
    let main_directories = ["CSV", "JSON", "MSP"];
    let sub_directories = ["NEG", "POS"];

    for main_dir in &main_directories {
        for sub_dir in &sub_directories {
            let dir_path = out_path.join(main_dir).join(sub_dir);
            if !dir_path.exists() {
                let _ = fs::create_dir_all(&dir_path);
            }
        }
    }

    // 5. Créer le dossier pour les spectres supprimés
    let deleted_spectrums_dir = out_path.join("DELETED_SPECTRUMS");
    if !deleted_spectrums_dir.exists() {
        let _ = fs::create_dir_all(&deleted_spectrums_dir);
    }

    Ok(())
}