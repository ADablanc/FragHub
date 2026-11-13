// src/splitter.rs
use pyo3::prelude::*;
use rayon::prelude::*;
use crate::spectrum::Spectrum;

/// Les 12 bibliothèques de sortie : POS / NEG × LC / GC / DI (injection directe) × expérimental / in silico.
/// Avant : 8 tables (LC / GC) passées une à une à chaque fonction d'écriture ; DI ajouté le 2026-11-11.
#[derive(Default)]
pub struct Libraries {
    pub pos_lc: Vec<Spectrum>, pub pos_lc_insilico: Vec<Spectrum>,
    pub pos_gc: Vec<Spectrum>, pub pos_gc_insilico: Vec<Spectrum>,
    pub pos_di: Vec<Spectrum>, pub pos_di_insilico: Vec<Spectrum>,
    pub neg_lc: Vec<Spectrum>, pub neg_lc_insilico: Vec<Spectrum>,
    pub neg_gc: Vec<Spectrum>, pub neg_gc_insilico: Vec<Spectrum>,
    pub neg_di: Vec<Spectrum>, pub neg_di_insilico: Vec<Spectrum>,
}

/// Séparation d'une bibliothèque.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Separation { Lc, Gc, Di }

impl Libraries {
    fn table(&mut self, pos: bool, sep: Separation, in_silico: bool) -> &mut Vec<Spectrum> {
        match (pos, sep, in_silico) {
            (true, Separation::Lc, false) => &mut self.pos_lc,
            (true, Separation::Lc, true) => &mut self.pos_lc_insilico,
            (true, Separation::Gc, false) => &mut self.pos_gc,
            (true, Separation::Gc, true) => &mut self.pos_gc_insilico,
            (true, Separation::Di, false) => &mut self.pos_di,
            (true, Separation::Di, true) => &mut self.pos_di_insilico,
            (false, Separation::Lc, false) => &mut self.neg_lc,
            (false, Separation::Lc, true) => &mut self.neg_lc_insilico,
            (false, Separation::Gc, false) => &mut self.neg_gc,
            (false, Separation::Gc, true) => &mut self.neg_gc_insilico,
            (false, Separation::Di, false) => &mut self.neg_di,
            (false, Separation::Di, true) => &mut self.neg_di_insilico,
        }
    }

    fn append(&mut self, other: &mut Libraries) {
        for (a, b) in self.tables_mut().into_iter().zip(other.tables_mut()) { a.append(b); }
    }

    fn tables_mut(&mut self) -> [&mut Vec<Spectrum>; 12] {
        [&mut self.pos_lc, &mut self.pos_lc_insilico, &mut self.pos_gc, &mut self.pos_gc_insilico,
         &mut self.pos_di, &mut self.pos_di_insilico, &mut self.neg_lc, &mut self.neg_lc_insilico,
         &mut self.neg_gc, &mut self.neg_gc_insilico, &mut self.neg_di, &mut self.neg_di_insilico]
    }

    /// Tous les spectres : LC et GC dans l'ordre d'avant (POS LC, POS GC, NEG LC, NEG GC, expérimental puis in
    /// silico), puis DI.
    pub fn all_spectra(&self) -> impl Iterator<Item = &Spectrum> {
        self.pos_lc.iter().chain(&self.pos_lc_insilico).chain(&self.pos_gc).chain(&self.pos_gc_insilico)
            .chain(&self.neg_lc).chain(&self.neg_lc_insilico).chain(&self.neg_gc).chain(&self.neg_gc_insilico)
            .chain(&self.pos_di).chain(&self.pos_di_insilico).chain(&self.neg_di).chain(&self.neg_di_insilico)
    }
}

/// Séparation d'un spectre : GC d'abord (règle is_gc_spectrum), puis DI (injection directe), sinon LC.
pub fn separation_of(spec: &Spectrum) -> Separation {
    let instr = spec.metadata.get("INSTRUMENTTYPE").map(|s| s.as_str()).unwrap_or("");
    let filename = crate::globals_vars::input_filename(&spec.metadata);
    if crate::globals_vars::is_gc_spectrum(filename, instr) { Separation::Gc }
    else if crate::globals_vars::is_di_spectrum(filename, instr) { Separation::Di }
    else { Separation::Lc }
}

/// Trie les spectres dans les 12 bibliothèques (LC/GC/DI, POS/NEG, IN_SILICO/EXP).
///
/// `par_iter().fold(...).reduce(...)` : chaque cœur trie son paquet (`fold`), puis les paquets sont fusionnés
/// (`reduce`) ; l'ordre des spectres est gardé dans chaque table.
pub fn master_splitter(
    py: Python,
    spectrum_list: Vec<Spectrum>,
    progress_callback: Option<PyObject>,
    total_items_callback: Option<PyObject>,
    prefix_callback: Option<PyObject>,
    item_type_callback: Option<PyObject>,
) -> PyResult<Libraries> {
    if let Some(cb) = &prefix_callback { cb.call1(py, ("Splitting spectra into categories:",))?; }
    if let Some(cb) = &item_type_callback { cb.call1(py, ("spectra",))?; }

    let total_items = spectrum_list.len();
    if let Some(cb) = &total_items_callback { cb.call1(py, (total_items, 0))?; }

    let result = py.allow_threads(|| {
        // Les spectres sont DÉPLACÉS dans les tables (et non copiés) : avant, la liste complète et sa copie triée
        // existaient en même temps. Même ordre qu'avant dans chaque table (fold puis reduce gardent l'ordre).
        spectrum_list
            .into_par_iter()
            .fold(Libraries::default, |mut acc, spec| {
                let ionmode = spec.metadata.get("IONMODE").map(|s| s.as_str()).unwrap_or("").to_lowercase();

                // Ni positif ni négatif : ne devrait plus arriver, check_for_bad_adduct supprime ces spectres
                // avec un motif (ionmode_missing_or_invalid.csv). Avant, ils disparaissaient ici sans trace.
                if ionmode == "positive" || ionmode == "negative" {
                    let is_pos = ionmode == "positive";
                    let sep = separation_of(&spec);
                    let pred = spec.metadata.get("PREDICTED").map(|s| s.as_str()).unwrap_or("").to_lowercase();
                    acc.table(is_pos, sep, pred == "true").push(spec);
                }
                acc
            })
            .reduce(Libraries::default, |mut a, mut b| {
                // On fusionne les résultats de tous les threads
                a.append(&mut b);
                a
            })
    });

    if let Some(cb) = &progress_callback { cb.call1(py, (total_items,))?; }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(ionmode: &str, instr: &str, pred: &str) -> Spectrum {
        let mut s = Spectrum::default();
        for (k, v) in [("IONMODE", ionmode), ("INSTRUMENTTYPE", instr), ("PREDICTED", pred), ("FILENAME", "x.msp")] {
            s.metadata.insert(k.to_string(), v.to_string());
        }
        s
    }

    #[test]
    fn douze_bibliotheques() {
        let mut libs = Libraries::default();
        for (m, t, p) in [("positive", "LC-ESI-QTOF", "false"), ("positive", "DI-ESI-QTOF", "false"), ("negative", "DI-PTR-TOF", "true"),
                          ("negative", "GC-EI-TOF", "false"), ("positive", "EI-B", "true")] {
            let s = spec(m, t, p);
            let sep = separation_of(&s);
            libs.table(m == "positive", sep, p == "true").push(s);
        }
        assert_eq!((libs.pos_lc.len(), libs.pos_di.len(), libs.neg_di_insilico.len(), libs.neg_gc.len(), libs.pos_gc_insilico.len()), (1, 1, 1, 1, 1));
        assert_eq!(libs.all_spectra().count(), 5);
    }

    #[test]
    fn tri_des_spectres_en_bibliotheques_ordre_garde() {
        let mut list = Vec::new();
        for (i, (m, t, p)) in [("positive", "LC-ESI-QTOF", "false"), ("POSITIVE", "LC-ESI-QTOF", "false"),
                               ("negative", "GC-EI-TOF", "true"), ("", "LC-ESI-QTOF", "false"),
                               ("positive", "LC-ESI-QTOF", "false")].into_iter().enumerate() {
            let mut s = spec(m, t, p);
            s.metadata.insert("NAME".into(), i.to_string());
            list.push(s);
        }
        let libs = crate::loaders::test_support::with_py(|py| master_splitter(py, list, None, None, None, None)).unwrap();
        let names: Vec<&str> = libs.pos_lc.iter().map(|s| s.metadata["NAME"].as_str()).collect();
        assert_eq!(names, vec!["0", "1", "4"]);   // mode écrit en majuscules accepté, ordre d'entrée gardé
        assert_eq!(libs.neg_gc_insilico.len(), 1);
        assert_eq!(libs.all_spectra().count(), 4);   // sans mode d'ionisation : écarté
    }

    #[test]
    fn separation_lue_aussi_dans_le_nom_du_fichier() {
        let mut s = spec("positive", "ESI-QTOF", "false");
        s.metadata.insert("FILENAME".into(), "MoNA_GC.msp".into());
        assert_eq!(separation_of(&s), Separation::Gc);
        s.metadata.insert("FILENAME".into(), "x.msp".into());
        assert_eq!(separation_of(&s), Separation::Lc);
    }
}
