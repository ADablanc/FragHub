use std::collections::HashMap;

/// Structure de données centrale représentant un spectre de masse.
///
/// Pour un développeur Python : C'est l'équivalent d'une `dataclass` ou d'un dictionnaire typé.
/// L'attribut `#[derive(...)]` demande au compilateur Rust de générer 
/// automatiquement pour nous le code pour copier (`.clone()`), afficher (`print!()` / `Debug`), 
/// et créer un spectre vide (`Spectrum::default()`). C'est extrêmement pratique !
#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    pub metadata: HashMap<String, String>,
    pub peaks: Vec<(f64, f64)>, // Toujours des tuples pour éviter la fragmentation RAM
}

/// Texte des pics tel qu'il était gardé dans `metadata["PEAKS_LIST"]` après le nettoyage
/// (`"{:.8} {:.8}"` par pic, séparés par des sauts de ligne). Il n'est plus gardé en mémoire (les pics étaient
/// stockés deux fois) : il est fabriqué seulement là où il est écrit (fichiers DELETED_SPECTRUMS des étapes qui
/// suivent le nettoyage) ou compté (choix du doublon gardé).
pub fn peaks_text(peaks: &[(f64, f64)]) -> String {
    let mut text = String::with_capacity(peaks.len() * 24);
    for (i, &(mz, int)) in peaks.iter().enumerate() {
        if i > 0 { text.push('\n'); }
        text.push_str(&format!("{:.8} {:.8}", mz, int));
    }
    text
}

/// Texte de `PEAKS_LIST` d'un spectre : celui des métadonnées s'il y est (formules du de novo), sinon celui des pics.
pub fn peaks_list_text(spec: &Spectrum) -> String {
    match spec.metadata.get("PEAKS_LIST") {
        Some(text) => text.clone(),
        None => peaks_text(&spec.peaks),
    }
}
