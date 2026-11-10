// src/spectra_to_msp
use crate::spectrum::Spectrum;
use std::fmt::Write;

// Fonction pour extraire et nettoyer une valeur depuis le dictionnaire Python
fn get_string(spec: &Spectrum, key: &str) -> String {
    if key == "PEAKS_LIST" {
        // --- NOUVEAU : On tente d'abord de récupérer la chaîne avec les formules De Novo ---
        if let Some(val) = spec.metadata.get("PEAKS_LIST") {
            if !val.trim().is_empty() && val != "NOT FOUND" {
                let sep = if val.contains(';') { ';' } else { '\n' };
                let lines_count = val.split(sep).filter(|s| !s.trim().is_empty()).count();

                // Si la chaîne contient des lettres (formules) et correspond au bon nombre de pics, on l'utilise
                if lines_count == spec.peaks.len() && val.chars().any(|c| c.is_ascii_alphabetic()) {
                    return val.replace(";", "\n"); // On s'assure d'avoir le format MSP
                }
            }
        }

        // --- COMPORTEMENT PAR DÉFAUT : Reconstruction rapide depuis les vecteurs ---
        if spec.peaks.is_empty() { return "NOT FOUND".to_string(); }
        let mut peaks_str = String::with_capacity(spec.peaks.len() * 20);
        for (i, &(mz, int)) in spec.peaks.iter().enumerate() {
            if i > 0 { peaks_str.push('\n'); }
            peaks_str.push_str(&format!("{} {}", mz, int));
        }
        return peaks_str;
    }
    if let Some(val) = spec.metadata.get(key) {
        let trimmed = val.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("nan") {
            return "NOT FOUND".to_string();
        }
        return val.clone();
    }
    "NOT FOUND".to_string()
}

/// Texte MSP d'un spectre (sans la ligne vide qui le sépare du suivant). Code inchangé, sorti de l'ancienne
/// conversion de tous les spectres d'un coup (spectra_to_msp_processing) : le texte est maintenant fabriqué au
/// moment de l'écriture, par paquets (writers/msp_writer.rs), au lieu d'être gardé en mémoire pour tous les
/// spectres (plusieurs Go sur une base complète, pendant toute l'écriture du CSV et du MSP).
pub(crate) fn msp_text(spec: &Spectrum) -> String {
    let mut spectrum = String::with_capacity(2048);

    let _ = write!(
        &mut spectrum,
        "NAME: {}\nPRECURSORMZ: {}\nPRECURSORTYPE: {}\nFORMULA: {}\nINCHIKEY: {}\nINCHI: {}\nSMILES: {}\nRT: {}\nIONMODE: {}\nINSTRUMENTTYPE: {}\nINSTRUMENT: {}\nCOLLISIONENERGY: {}\nEXACTMASS: {}\nIONIZATION: {}\nMSLEVEL: {}\nDISSOCIATIONMETHOD: {}\nAVERAGEMASS: {}\nENTROPY: {}\nCLASSYFIRE_SUPERCLASS: {}\nCLASSYFIRE_CLASS: {}\nCLASSYFIRE_SUBCLASS: {}\nNPCLASS_PATHWAY: {}\nNPCLASS_SUPERCLASS: {}\nNPCLASS_CLASS: {}\nCOMMENT: FILENAME={}; DATABASE_NAME={}; FILEHASH={}; PREDICTED={}; SPLASH={}; SPECTRUMID={}; RESOLUTION={}; SYNON={}\nNUM PEAKS: {}\n",
        get_string(spec, "NAME"),
        get_string(spec, "PRECURSORMZ"),
        get_string(spec, "PRECURSORTYPE"),
        get_string(spec, "FORMULA"),
        get_string(spec, "INCHIKEY"),
        get_string(spec, "INCHI"),
        get_string(spec, "SMILES"),
        get_string(spec, "RT"),
        get_string(spec, "IONMODE"),
        get_string(spec, "INSTRUMENTTYPE"),
        get_string(spec, "INSTRUMENT"),
        get_string(spec, "COLLISIONENERGY"),
        get_string(spec, "EXACTMASS"),
        get_string(spec, "IONIZATION"),
        get_string(spec, "MSLEVEL"),
        get_string(spec, "FRAGMENTATIONMETHODE"), // Note: internal col is still FRAGMENTATIONMETHODE
        get_string(spec, "AVERAGEMASS"),
        get_string(spec, "ENTROPY"),
        get_string(spec, "CLASSYFIRE_SUPERCLASS"),
        get_string(spec, "CLASSYFIRE_CLASS"),
        get_string(spec, "CLASSYFIRE_SUBCLASS"),
        get_string(spec, "NPCLASS_PATHWAY"),
        get_string(spec, "NPCLASS_SUPERCLASS"),
        get_string(spec, "NPCLASS_CLASS"),
        get_string(spec, "FILENAME"), get_string(spec, "DATABASE_NAME"), get_string(spec, "FILEHASH"), get_string(spec, "PREDICTED"),
        get_string(spec, "SPLASH"), get_string(spec, "SPECTRUMID"), get_string(spec, "RESOLUTION"), get_string(spec, "SYNON"),
        get_string(spec, "NUM PEAKS")
    );

    let peaks = get_string(spec, "PEAKS_LIST");
    if peaks != "NOT FOUND" { spectrum.push_str(&peaks); }
    spectrum.push('\n');

    spectrum
}
