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

#[cfg(test)]
mod tests {
    use super::*;

    fn spectrum(fields: &[(&str, &str)], peaks: Vec<(f64, f64)>) -> Spectrum {
        let mut s = Spectrum { peaks, ..Default::default() };
        for (k, v) in fields { s.metadata.insert(k.to_string(), v.to_string()); }
        s
    }

    #[test]
    fn champs_absents_ou_nan_ecrits_not_found() {
        let text = msp_text(&spectrum(&[("NAME", "Caffeine"), ("RT", "nan"), ("SMILES", "  ")], vec![(138.06, 1.0)]));
        assert!(text.starts_with("NAME: Caffeine\nPRECURSORMZ: NOT FOUND\n"));
        assert!(text.contains("\nRT: NOT FOUND\n") && text.contains("\nSMILES: NOT FOUND\n"));
        assert!(text.ends_with("NUM PEAKS: NOT FOUND\n138.06 1\n"));
    }

    #[test]
    fn methode_de_fragmentation_et_commentaire() {
        let text = msp_text(&spectrum(&[("FRAGMENTATIONMETHODE", "HCD"), ("FILENAME", "a.msp"), ("SPLASH", "splash10-x")], vec![]));
        assert!(text.contains("\nDISSOCIATIONMETHOD: HCD\n"));
        assert!(text.contains("\nCOMMENT: FILENAME=a.msp; DATABASE_NAME=NOT FOUND; FILEHASH=NOT FOUND; PREDICTED=NOT FOUND; SPLASH=splash10-x;"));
        assert!(text.ends_with("NUM PEAKS: NOT FOUND\n\n"));   // aucun pic : rien après NUM PEAKS
    }

    #[test]
    fn pics_avec_formules_de_novo() {
        let peaks = vec![(110.07, 50.0), (138.06, 100.0)];
        // formules du de novo, autant de lignes que de pics : reprises (« ; » devenu saut de ligne)
        let s = spectrum(&[("PEAKS_LIST", "110.07 50 C6H8NO+/0.5;138.06 100 C7H8NO+/1.2")], peaks.clone());
        assert!(msp_text(&s).ends_with("110.07 50 C6H8NO+/0.5\n138.06 100 C7H8NO+/1.2\n"));
        // nombre de lignes différent du nombre de pics : pics reconstruits
        let s = spectrum(&[("PEAKS_LIST", "110.07 50 C6H8NO+/0.5")], peaks.clone());
        assert!(msp_text(&s).ends_with("110.07 50\n138.06 100\n"));
        // texte sans formule : pics reconstruits
        let s = spectrum(&[("PEAKS_LIST", "110.07 50\n138.06 100")], peaks);
        assert!(msp_text(&s).ends_with("110.07 50\n138.06 100\n"));
    }
}
