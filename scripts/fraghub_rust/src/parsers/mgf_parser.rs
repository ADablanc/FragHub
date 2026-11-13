// src/parsers/mgf_parser.rs

use pyo3::prelude::*;
use crate::spectrum::Spectrum;
use rayon::prelude::*;
use std::collections::HashMap;
use crate::parsers::raw_metadata::RawMetadata;

// 1. La structure pour passer les données entre les cœurs CPU sans bloquer Python
/// Structure interne pour le multithreading.
///
/// Pour un développeur Python : En Python, les objets créés dans différents processus
/// doivent être sérialisés (Pickle) pour être renvoyés au processus principal, ce qui est très lent.
/// En Rust, on passe cette structure entre les threads (cœurs CPU) de manière ultra-rapide
/// et sans copie superflue de la mémoire.
struct ParsedSpectrum {
    metadata: RawMetadata, // champs bruts, dans l'ordre du fichier
    peaks: Vec<Vec<f64>>,
}

/// Fonction principale de parsing MGF via Rayon.
///
/// # Arguments
/// * `py` (Python) : Le token PyO3.
/// * `final_mgf` (Vec<String>) : Les spectres MGF bruts.
/// * `keys_dict` (HashMap<String, String>) : Mapping des clés.
/// * `keys_list` (Vec<String>) : Liste des clés à conserver.
/// * `progress_callback` (Option<PyObject>) : Callback de progression.
/// * `total_items_callback` (Option<PyObject>) : Callback de total.
/// * `prefix_callback` (Option<PyObject>) : Callback du préfixe.
/// * `item_type_callback` (Option<PyObject>) : Callback du type.
///
/// # Returns
/// * `PyResult<Vec<Spectrum>>` : La liste de spectres parsés.
pub fn mgf_to_dict_processing(
    py: Python,
    final_mgf: Vec<String>,
    keys_dict: HashMap<String, String>,
    keys_list: Vec<String>,
    db_name: String,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Vec<Spectrum>> {
    let total = final_mgf.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total, 0))?; }
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Parsing MGF spectrums:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }

    let mut result_list = Vec::new();
    let mut processed = 0;

    // Découpage par paquets (chunks) comme dans votre version Python
    let chunk_size = 2000;

    // Textes bruts consommés paquet par paquet : chaque paquet est libéré dès qu'il est lu (avant, tout le fichier
    // restait en mémoire jusqu'à la fin de la lecture, en plus des spectres lus)
    let mut remaining = final_mgf.into_iter();
    loop {
        crate::stop::check()?; // arrêt demandé (stop.rs)
        let chunk: Vec<String> = remaining.by_ref().take(chunk_size).collect();
        if chunk.is_empty() {
            break;
        }

        // ====================================================================
        // ÉTAPE 1 : MULTITHREADING PUR RUST (Équivalent de ThreadPoolExecutor)
        // ====================================================================
        let parsed_chunk: Vec<ParsedSpectrum> = chunk.par_iter().filter_map(|spectrum| {
            // Attention : Assurez-vous d'avoir bien retiré le "?" dans globals_vars.rs
            // pour METADATA_PEAK_LIST_SPLIT_PATTERN_MGF comme discuté précédemment !
            {
                // Spectre sans pics : métadonnées lues, liste de pics vide ; il est ensuite supprimé avec le motif
                // « peaks list is empty » (écrit dans DELETED_SPECTRUMS) au lieu d'être écarté ici sans trace.
                let caps = crate::globals_vars::METADATA_PEAK_LIST_SPLIT_PATTERN_MGF.captures(spectrum);
                let peaks_str = caps.as_ref().and_then(|c| c.get(2)).map_or("", |m| m.as_str());
                // Sans pics, tout le bloc (jusqu'à sa dernière ligne, même sans retour à la ligne final)
                let metadata_str = if peaks_str.trim().is_empty() { spectrum.as_str() }
                    else { caps.as_ref().and_then(|c| c.get(1)).map_or(spectrum.as_str(), |m| m.as_str()) };

                if !metadata_str.trim().is_empty() {
                    let mut metadata_dict = RawMetadata::default();
                    let mut real_fields = 0usize;

                    // Extraction des métadonnées
                    for m in crate::globals_vars::METADATA_PATTERN_MGF.captures_iter(metadata_str) {
                        let k_raw = m.get(1).unwrap().as_str();
                        let v_raw = m.get(2).unwrap().as_str();

                        let k = crate::globals_vars::METADATA_FIELDS_NAME_PATTERN.replace_all(k_raw, "").to_lowercase().trim().to_string();
                        if k != "filename" && k != "filehash" { real_fields += 1; }
                        let v = crate::globals_vars::METADATA_STRIP_VALUE_PATTERN.replace_all(v_raw, "").to_string();
                        metadata_dict.insert(k, v);
                    }

                    // Au moins un champ en plus du nom et de l'empreinte du fichier (ajoutés par le lecteur)
                    if real_fields > 0 {
                        let mut peaks_array = Vec::new();

                        // Extraction des pics
                        for (mz, int) in crate::globals_vars::peak_pairs(peaks_str) {
                            let mz = mz.replace(",", ".").parse::<f64>().unwrap_or(0.0);
                            let int = int.replace(",", ".").parse::<f64>().unwrap_or(0.0);
                            peaks_array.push(vec![mz, int]);
                        }

                        // Métadonnées présentes : spectre gardé, même sans pics (supprimé plus tard avec son motif)
                        return Some(ParsedSpectrum { metadata: metadata_dict, peaks: peaks_array });
                    }
                }
            }
            None // Équivalent de "if not metadata or not peak_list: return None"
        }).collect();

        // ====================================================================
        // ÉTAPE 2 : Mapping et création du dictionnaire (RUST NATIVE)
        // ====================================================================
        for parsed in parsed_chunk {
            let mut spec = Spectrum::default();

            // Renommage vers les clés FragHub : si deux champs donnent la même clé, le premier du fichier gagne
            parsed.metadata.map_into(&keys_dict, |mapped| keys_list.iter().any(|k| k == mapped), &mut spec.metadata);
            spec.metadata.insert("DATABASE_NAME".to_string(), db_name.clone());

            // Ajout de la liste de pics
            spec.peaks = parsed.peaks.into_iter().map(|p| (p[0], p[1])).collect();

            // Complétion des clés manquantes
            for key in &keys_list {
                if !spec.metadata.contains_key(key) && key != "PEAKS_LIST" { 
                    spec.metadata.insert(key.clone(), "".to_string()); 
                }
            }

            result_list.push(spec);
        }

        // ====================================================================
        // ÉTAPE 3 : MISE À JOUR DE LA BARRE DE PROGRESSION VUE.JS
        // ====================================================================
        processed += chunk.len();
        if let Some(cb) = &progress_callback { cb.call1(py, (processed,))?; }
    }

    Ok(result_list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::test_support::with_py;

    /// Spectres lus à partir de blocs tels que les donne le lecteur MGF (nom et empreinte du fichier en tête,
    /// sans la ligne END IONS)
    fn parse(blocks: &[&str]) -> Vec<Spectrum> {
        let keys: HashMap<String, String> = [("filename", "FILENAME"), ("filehash", "FILEHASH"), ("name", "NAME"),
            ("pepmass", "PRECURSORMZ"), ("spectrumid", "SPECTRUMID"), ("smiles", "SMILES"), ("ionmode", "IONMODE"),
            ("scans", "SCANS")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let list: Vec<String> = ["FILENAME", "FILEHASH", "NAME", "PRECURSORMZ", "SPECTRUMID", "SMILES", "IONMODE", "RT",
            "PEAKS_LIST"].iter().map(|s| s.to_string()).collect();
        let blocks: Vec<String> = blocks.iter().map(|b| format!("FILENAME=lib.mgf\nFILEHASH=h\nBEGIN IONS\n{b}")).collect();
        with_py(|py| mgf_to_dict_processing(py, blocks, keys, list, "GNPS".to_string(), None, None, None, None)).unwrap()
    }

    #[test]
    fn champs_renommes_et_pics_lus() {
        // Extrait réel du MGF GNPS (CCMSLIB00000001547) ; pics séparés par un espace ou une tabulation
        let out = parse(&["PEPMASS=981.54\nCHARGE=0\nMSLEVEL=2\nIONMODE=Positive\nNAME=3-Des-Microcystein_LR M+H\nSMILES=CC(C)CC1NC(=O)C(C)NC(=O)C(=C)N(C)C(=O)CCC\nSPECTRUMID=CCMSLIB00000001547\nSCANS=1\n289.286377\t8068.0\n295.545288 22507.0\n298.489624 3925.0"]);
        assert_eq!(out.len(), 1);
        let m = &out[0].metadata;
        assert_eq!(m["NAME"], "3-Des-Microcystein_LR M+H");
        assert_eq!(m["PRECURSORMZ"], "981.54");
        assert_eq!(m["SPECTRUMID"], "CCMSLIB00000001547");
        assert_eq!(m["IONMODE"], "Positive");
        assert_eq!(m["FILENAME"], "lib.mgf");
        assert_eq!(m["FILEHASH"], "h");
        assert_eq!(m["DATABASE_NAME"], "GNPS");
        // Champ attendu mais absent du fichier : vide. Champs hors liste (CHARGE, MSLEVEL, SCANS) : pas gardés
        assert_eq!(m["RT"], "");
        assert!(!m.contains_key("SCANS") && !m.contains_key("CHARGE") && !m.contains_key("MSLEVEL"));
        assert_eq!(out[0].peaks, vec![(289.286377, 8068.0), (295.545288, 22507.0), (298.489624, 3925.0)]);
    }

    #[test]
    fn pics_en_ecriture_scientifique() {
        // Majuscule ou minuscule, avec ou sans signe ; virgule décimale dans l'intensité ; « m/z:intensité »
        let out = parse(&["NAME=X\n1.0012E+02 1.5E+05\n2.5e2 3e-1\n300 2E3\n350.5 12,5\n400.25:7"]);
        assert_eq!(out[0].peaks, vec![(100.12, 150000.0), (250.0, 0.3), (300.0, 2000.0), (350.5, 12.5), (400.25, 7.0)]);
        // Virgule décimale dans le m/z aussi (avant : aucun pic lu, spectre supprimé)
        let out = parse(&["NAME=X\n56,0495 20\n69,0447\t15,5\n83.0604 10"]);
        assert_eq!(out[0].peaks, vec![(56.0495, 20.0), (69.0447, 15.5), (83.0604, 10.0)]);
    }

    #[test]
    fn spectre_sans_pics_garde_pour_etre_supprime_avec_son_motif() {
        // Métadonnées sans pics : spectre gardé avec une liste vide (supprimé ensuite, « peaks list is empty »)
        let out = parse(&["NAME=X\nSPECTRUMID=CCMSLIB1", "NAME=Y\nPEPMASS=100.5\n50.1 3"]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].metadata["SPECTRUMID"], "CCMSLIB1");
        assert!(out[0].peaks.is_empty());
        assert_eq!(out[1].peaks, vec![(50.1, 3.0)]);
        // Bloc sans aucun champ (seulement le nom et l'empreinte ajoutés par le lecteur) : pas un spectre
        assert!(parse(&[""]).is_empty());
        assert!(parse(&["50.1 3\n60.2 4"]).is_empty());
    }

    #[test]
    fn un_spectre_par_bloc_dans_l_ordre_du_fichier() {
        // Plus de blocs qu'un paquet de lecture (2000) : aucun perdu, ordre gardé
        let blocks: Vec<String> = (0..4100).map(|i| format!("NAME=S{i}\n{}.5 1", i + 50)).collect();
        let refs: Vec<&str> = blocks.iter().map(|s| s.as_str()).collect();
        let out = parse(&refs);
        assert_eq!(out.len(), 4100);
        assert!(out.iter().enumerate().all(|(i, s)| s.metadata["NAME"] == format!("S{i}") && s.peaks == vec![(i as f64 + 50.5, 1.0)]));
    }
}
