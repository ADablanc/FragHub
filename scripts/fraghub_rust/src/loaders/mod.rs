// src/loaders/mod.rs
//! Lecture des fichiers d'entrée, un fichier par format : chaque lecteur découpe le fichier en
//! spectres bruts (textes), ensuite analysés par les parseurs (module parsers : msp_parser.rs, mgf_parser.rs, json_parser.rs).
//! - `json_loader.rs`  : JSON en tableau (format GNPS) ;
//! - `jsonl_loader.rs` : JSONL (un spectre JSON par ligne) ;
//! - `msp_loader.rs`   : MSP ;
//! - `mgf_loader.rs`   : MGF.
//! (Le CSV est lu et analysé en une seule étape, dans parsers/csv_parser.rs.)

use pyo3::prelude::*;
use sha2::{Sha256, Digest};

mod json_loader;
mod jsonl_loader;
mod msp_loader;
mod mgf_loader;

pub use json_loader::load_spectrum_list_json;
pub use jsonl_loader::load_spectrum_list_json_2;
pub use msp_loader::load_spectrum_list_from_msp;
pub use mgf_loader::load_spectrum_list_from_mgf;

/// Clés ajoutées par les lecteurs JSON / JSONL en tête de chaque spectre : nom et empreinte du fichier.
/// Préfixe à part pour ne jamais se confondre avec une clé du fichier (avant : "filename" et "filehash",
/// remplacés par une clé "filename" présente dans le spectre).
pub const INJECTED_PREFIX: &str = "__fraghub_";
pub const INJECTED_FILENAME: &str = "__fraghub_filename";
pub const INJECTED_FILEHASH: &str = "__fraghub_filehash";

/// Gets the size of a file in bytes, converts it to a string, and returns the SHA-256 hash.
///
/// Pour un développeur Python : La macro `#[pyfunction]` indique à PyO3 que cette fonction
/// Rust doit être compilée et exposée comme une fonction appelable directement depuis Python.
///
/// # Arguments
/// * `file_path` (&str) : Le chemin du fichier.
///
/// # Returns
/// * `String` : Le hash SHA-256 de la taille du fichier.
#[pyfunction]
pub fn generate_file_hash(file_path: &str) -> String {
    if let Ok(metadata) = std::fs::metadata(file_path) {
        let size = metadata.len().to_string();
        let mut hasher = Sha256::new();
        hasher.update(size.as_bytes());
        format!("{:x}", hasher.finalize())
    } else {
        format!("Error: File not found at {}", file_path)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    /// Interpréteur Python des tests : les lecteurs prennent le jeton `Python` (barres de progression), même quand
    /// aucun callback n'est donné.
    pub fn with_py<R>(f: impl FnOnce(pyo3::Python) -> R) -> R {
        pyo3::prepare_freethreaded_python();
        pyo3::Python::with_gil(f)
    }

    /// Fichier temporaire propre au test (dossier par processus et par nom de test)
    pub fn temp_file(test: &str, name: &str, content: &[u8]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("fraghub_{}_{}", test, std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{temp_file, with_py};

    #[test]
    fn empreinte_du_fichier_selon_sa_taille() {
        // SHA-256 du texte de la taille (« 5 »), pas du contenu : deux fichiers de même taille ont la même empreinte
        let a = temp_file("hash", "a.msp", b"abcde");
        let b = temp_file("hash", "b.msp", b"vwxyz");
        let attendu = format!("{:x}", Sha256::digest(b"5"));
        assert_eq!(generate_file_hash(a.to_str().unwrap()), attendu);
        assert_eq!(generate_file_hash(b.to_str().unwrap()), attendu);
        assert!(generate_file_hash("/chemin/absent.msp").starts_with("Error: File not found"));
    }

    #[test]
    fn mgf_un_texte_par_spectre_avec_nom_et_empreinte() {
        let p = temp_file("mgf", "lib.mgf", b"BEGIN IONS\nTITLE=A\nPEPMASS=100.5\n50 1\n\n60 2\nEND IONS\n\nBEGIN IONS\nTITLE=B\n70 3\nEND IONS\n");
        let hash = generate_file_hash(p.to_str().unwrap());
        let out = with_py(|py| load_spectrum_list_from_mgf(py, p.to_str().unwrap(), None, None, None, None)).unwrap();
        assert_eq!(out.len(), 2);
        // ligne vide entre les pics ignorée : les pics suivants ne sont plus perdus
        assert_eq!(out[0], format!("FILENAME=lib.mgf\nFILEHASH={hash}\nBEGIN IONS\nTITLE=A\nPEPMASS=100.5\n50 1\n60 2"));
        assert!(out[1].ends_with("TITLE=B\n70 3"));
    }

    #[test]
    fn jsonl_une_ligne_par_spectre() {
        let p = temp_file("jsonl", "lib.jsonl", b"{\"name\": \"A\"}\n\n   \n{\"name\": \"B\", \"peaks\": [[1, 2]]}\nligne qui n'est pas un objet\n");
        let hash = generate_file_hash(p.to_str().unwrap());
        let out = with_py(|py| load_spectrum_list_json_2(py, p.to_str().unwrap(), None, None, None, None)).unwrap();
        assert_eq!(out, vec![
            format!("{{\"{INJECTED_FILENAME}\":\"lib.jsonl\",\"{INJECTED_FILEHASH}\":\"{hash}\",\"name\": \"A\"}}"),
            format!("{{\"{INJECTED_FILENAME}\":\"lib.jsonl\",\"{INJECTED_FILEHASH}\":\"{hash}\",\"name\": \"B\", \"peaks\": [[1, 2]]}}"),
        ]);
    }

    #[test]
    fn json_tableau_objets_de_premier_niveau() {
        // accolades et guillemets échappés dans les textes, objets imbriqués : seuls les objets du tableau comptent
        let content = br#"[
  {"name": "A {pas une accolade}", "c": "guillemet \" puis }", "sous": {"x": 1}},
  {"name": "B"}
]"#;
        let p = temp_file("json", "lib.json", content);
        let out = with_py(|py| load_spectrum_list_json(py, p.to_str().unwrap(), None, None, None, None)).unwrap();
        assert_eq!(out.len(), 2);
        let first: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(first["name"], "A {pas une accolade}");
        assert_eq!(first["c"], "guillemet \" puis }");
        assert_eq!(first["sous"]["x"], 1);
        assert_eq!(first[INJECTED_FILENAME], "lib.json");
        let second: serde_json::Value = serde_json::from_str(&out[1]).unwrap();
        assert_eq!(second["name"], "B");
    }

    #[test]
    fn arret_demande_pendant_la_lecture() {
        let p = temp_file("stop", "lib.mgf", b"BEGIN IONS\n50 1\nEND IONS\n");
        let _seul = crate::stop::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::stop::reset_stop();
        crate::stop::request();
        let r = with_py(|py| load_spectrum_list_from_mgf(py, p.to_str().unwrap(), None, None, None, None));
        crate::stop::reset_stop();
        assert!(r.is_err());
    }
}
