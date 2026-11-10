// src/stop.rs
//! Arrêt demandé par l'utilisateur (bouton stop de l'interface, Ctrl+C dans le terminal).
//!
//! Un seul indicateur, partagé par tous les threads (`AtomicBool` : lecture et écriture sûres depuis n'importe
//! quel thread, sans verrou, pour un coût négligeable). Toutes les boucles longues le regardent (à chaque paquet
//! de spectres, chaque morceau de fichier lu, chaque molécule RDKit...) : l'arrêt a lieu en moins d'une seconde,
//! quelle que soit l'étape. Avant, l'arrêt n'était vu qu'entre deux étapes ou à un appel de la barre de progression.
//! Ce qui a déjà été écrit dans le dossier de sortie est remis comme avant le run (voir output_guard.rs).
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Message d'arrêt, attendu tel quel par FragHub.py
pub const STOP_MESSAGE: &str = "Process stopped by user.";

static STOP: AtomicBool = AtomicBool::new(false);
static CTRL_C_PRESSES: AtomicUsize = AtomicUsize::new(0);

/// Vrai si l'arrêt a été demandé.
#[inline]
pub fn requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

/// Demande l'arrêt (appelable depuis n'importe quel thread).
pub fn request() {
    STOP.store(true, Ordering::Relaxed);
}

/// Erreur Python d'arrêt (même message qu'avant).
pub fn error() -> PyErr {
    PyException::new_err(STOP_MESSAGE)
}

/// `Err(arrêt)` si l'arrêt a été demandé.
#[inline]
pub fn check() -> PyResult<()> {
    if requested() { Err(error()) } else { Ok(()) }
}

/// Même chose pour le code qui renvoie des erreurs en texte (writers).
#[inline]
pub fn check_str() -> Result<(), String> {
    if requested() { Err(STOP_MESSAGE.to_string()) } else { Ok(()) }
}

/// Erreur d'un writer convertie en erreur Python : l'arrêt reste un arrêt (et non une erreur d'écriture).
pub fn io_error(e: String) -> PyErr {
    if e == STOP_MESSAGE { error() } else { pyo3::exceptions::PyIOError::new_err(e) }
}

/// Bouton stop de l'interface : demande l'arrêt du run en cours.
#[pyfunction]
pub fn request_stop() {
    request();
}

/// Remet l'indicateur à zéro (au début de chaque run).
#[pyfunction]
pub fn reset_stop() {
    STOP.store(false, Ordering::Relaxed);
    CTRL_C_PRESSES.store(0, Ordering::Relaxed);
}

/// Ctrl+C dans le terminal (mode CLI) : le premier demande un arrêt propre (dossier de sortie remis comme avant le
/// run), le deuxième arrête le programme tout de suite. À n'appeler qu'en CLI : dans l'application, Ctrl+C doit
/// continuer d'arrêter le serveur.
#[pyfunction]
pub fn install_ctrl_c_handler() {
    // Une seule fois par processus (une deuxième installation renvoie une erreur, sans effet)
    let _ = ctrlc::set_handler(|| {
        if CTRL_C_PRESSES.fetch_add(1, Ordering::Relaxed) == 0 {
            request();
            eprintln!("\n[FragHub] Arrêt demandé : fin de la tâche en cours et remise du dossier de sortie comme avant \
                       le run... (Ctrl+C encore une fois pour quitter tout de suite)");
        } else {
            std::process::exit(130);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicateur_partage_entre_threads() {
        reset_stop();
        assert!(check().is_ok());
        std::thread::spawn(request).join().unwrap();
        assert!(requested());
        assert_eq!(check_str().unwrap_err(), STOP_MESSAGE);
        reset_stop();
        assert!(!requested());
    }
}
