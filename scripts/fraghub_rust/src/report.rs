// src/report.rs
//! Rapport HTML de fin de run (`report_<date>.html` dans le dossier de sortie).
//!
//! Toutes les données du rapport sont réunies dans UN objet JSON (`{REPORT_DATA}` du gabarit
//! report_template.html) : le gabarit en fait les tableaux (JavaScript sans bibliothèque, lisibles hors ligne) et les
//! graphiques (Plotly et ECharts, chargés depuis Internet). Les nombres sont calculés ici, une seule fois.
use pyo3::prelude::*;
use crate::spectrum::Spectrum;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Nombre de spectres et nombre d'InChIKey différentes d'une bibliothèque.
fn get_df_stats(list: &[Spectrum]) -> (usize, usize) {
    let unique: HashSet<&str> = list.iter().filter_map(|spec| spec.metadata.get("INCHIKEY"))
        .map(|s| s.as_str()).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("nan")).collect();
    (list.len(), unique.len())
}

/// Mots avec une majuscule initiale (noms de marques et d'instruments de l'arbre) ; vide ou inconnu : « NOT FOUND ».
fn capitalize_words(s: &str) -> String {
    if s.to_uppercase() == "NOT FOUND" || s.to_uppercase() == "UNKNOWN" {
        return "NOT FOUND".to_string();
    }
    s.split_whitespace()
     .map(|word| {
         let mut c = word.chars();
         match c.next() {
             None => String::new(),
             Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
         }
     })
     .collect::<Vec<String>>()
     .join(" ")
}

/// Valeur de classe lisible : vide, « nan », « unknown » ou « NOT FOUND » donnent « Unknown ».
fn class_value(s: Option<&String>) -> String {
    let t = s.map(|s| s.trim().trim_matches('"').trim()).unwrap_or("");
    if t.is_empty() || t.eq_ignore_ascii_case("nan") || t.eq_ignore_ascii_case("unknown") || t.eq_ignore_ascii_case("not found") {
        "Unknown".to_string()
    } else {
        t.to_string()
    }
}

/// Hiérarchie (racine → niveaux) en listes ids / labels / parents / values pour un sunburst ou un arbre.
#[derive(Default)]
struct Hierarchy {
    counts: BTreeMap<String, usize>,
    labels: HashMap<String, String>,
    parents: HashMap<String, String>,
}

impl Hierarchy {
    fn add(&mut self, root: &str, levels: &[String]) {
        *self.counts.entry(root.to_string()).or_insert(0) += 1;
        self.labels.insert(root.to_string(), root.to_string());
        self.parents.insert(root.to_string(), String::new());
        let mut parent = root.to_string();
        for level in levels {
            let id = format!("{parent}|{level}");
            *self.counts.entry(id.clone()).or_insert(0) += 1;
            self.labels.insert(id.clone(), level.clone());
            self.parents.insert(id.clone(), parent.clone());
            parent = id;
        }
    }

    fn to_json(&self) -> Value {
        let ids: Vec<&String> = self.counts.keys().collect();
        json!({
            "ids": ids,
            "labels": ids.iter().map(|id| self.labels[*id].clone()).collect::<Vec<_>>(),
            "parents": ids.iter().map(|id| self.parents[*id].clone()).collect::<Vec<_>>(),
            "values": ids.iter().map(|id| self.counts[*id]).collect::<Vec<_>>(),
        })
    }
}

/// Format d'un fichier d'entrée d'après son extension.
fn input_format(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn generate_report_processing(
    _py: Python,
    output_directory: String,
    date_str: String,
    total_time_str: String,
    parameters_dict: &HashMap<String, f64>,
    input_paths: &Vec<String>,
    input_db_names: &HashMap<String, String>,
    deletion_report: &crate::deletion_report::DeletionReport,
    libs: &crate::splitter::Libraries,
) -> PyResult<()> {
    let html = build_report_html(&output_directory, &date_str, &total_time_str, parameters_dict, input_paths,
                                 input_db_names, deletion_report, libs);
    let file_name = format!("report_{}.html", date_str);
    let report_path = Path::new(&output_directory).join(file_name);
    // Noté avant écriture : retiré si le run s'arrête (output_guard.rs)
    crate::output_guard::prepare(&report_path, false)?;
    let mut file = File::create(report_path)?;
    file.write_all(html.as_bytes())?;
    Ok(())
}

/// Texte HTML complet du rapport (sans écriture : testable).
#[allow(clippy::too_many_arguments)]
fn build_report_html(
    output_directory: &str,
    date_str: &str,
    total_time_str: &str,
    parameters_dict: &HashMap<String, f64>,
    input_paths: &[String],
    input_db_names: &HashMap<String, String>,
    deletion_report: &crate::deletion_report::DeletionReport,
    libs: &crate::splitter::Libraries,
) -> String {
    let data = report_data(output_directory, date_str, total_time_str, parameters_dict, input_paths, input_db_names,
                           deletion_report, libs);
    // « </ » dans une chaîne (nom de fichier...) fermerait la balise <script> : écrit « <\/ », même valeur en JSON
    let data_text = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()).replace("</", "<\\/");
    include_str!("report_template.html").replace("{REPORT_DATA}", &data_text)
}

#[allow(clippy::too_many_arguments)]
fn report_data(
    output_directory: &str,
    date_str: &str,
    total_time_str: &str,
    parameters_dict: &HashMap<String, f64>,
    input_paths: &[String],
    input_db_names: &HashMap<String, String>,
    d: &crate::deletion_report::DeletionReport,
    libs: &crate::splitter::Libraries,
) -> Value {
    let get_bool = |key: &str| parameters_dict.get(key).cloned().unwrap_or(0.0) == 1.0;
    let get_f64 = |key: &str| parameters_dict.get(key).cloned().unwrap_or(0.0);

    // --- En-tête ---
    let parts: Vec<&str> = date_str.split("__").collect();
    let display_date = if parts.len() == 2 {
        format!("{} {}", parts[0].replace('_', "-"), parts[1].replace('_', ":"))
    } else {
        date_str.replace('_', " ")
    };
    let (db_version, db_doi) = crate::global_state::STATE.read()
        .map(|s| (s.internal_db_version.clone(), s.internal_db_doi.clone())).unwrap_or_default();
    let meta = json!({
        "date": display_date,
        "duration": total_time_str,
        "fraghub_version": env!("CARGO_PKG_VERSION"),
        "rdkit_version": crate::rdkit_native::rdkit_version(),
        "database_version": db_version,
        "database_doi": db_doi,
        "output_directory": output_directory,
    });

    // --- Bibliothèques de sortie (12) ---
    let library_list: [(&str, &str, &str, &[Spectrum]); 12] = [
        ("LC", "POS", "Experimental", &libs.pos_lc), ("LC", "POS", "In silico", &libs.pos_lc_insilico),
        ("LC", "NEG", "Experimental", &libs.neg_lc), ("LC", "NEG", "In silico", &libs.neg_lc_insilico),
        ("GC", "POS", "Experimental", &libs.pos_gc), ("GC", "POS", "In silico", &libs.pos_gc_insilico),
        ("GC", "NEG", "Experimental", &libs.neg_gc), ("GC", "NEG", "In silico", &libs.neg_gc_insilico),
        ("DI", "POS", "Experimental", &libs.pos_di), ("DI", "POS", "In silico", &libs.pos_di_insilico),
        ("DI", "NEG", "Experimental", &libs.neg_di), ("DI", "NEG", "In silico", &libs.neg_di_insilico),
    ];
    let libraries: Vec<Value> = library_list.iter().map(|(sep, mode, kind, list)| {
        let (n, u) = get_df_stats(list);
        json!({"separation": sep, "ionmode": mode, "type": kind, "spectra": n, "inchikeys": u})
    }).collect();
    // InChIKey différentes par groupe (une molécule de deux bibliothèques ne compte qu'une fois)
    let unique_in = |filter: &dyn Fn(&str, &str, &str) -> bool| -> usize {
        let mut keys: HashSet<&str> = HashSet::new();
        for (sep, mode, kind, list) in library_list.iter() {
            if !filter(sep, mode, kind) { continue; }
            for spec in list.iter() {
                if let Some(k) = spec.metadata.get("INCHIKEY") {
                    if !k.is_empty() && !k.eq_ignore_ascii_case("nan") { keys.insert(k.as_str()); }
                }
            }
        }
        keys.len()
    };
    let mut groups = serde_json::Map::new();
    for sep in ["LC", "GC", "DI"] {
        groups.insert(sep.to_string(), json!(unique_in(&|s, _, _| s == sep)));
        for mode in ["POS", "NEG"] {
            groups.insert(format!("{sep}|{mode}"), json!(unique_in(&|s, m, _| s == sep && m == mode)));
        }
    }
    let total_kept: usize = library_list.iter().map(|(_, _, _, l)| l.len()).sum();
    let total_unique = unique_in(&|_, _, _| true);

    // --- Suppressions ---
    let deletions = vec![
        ("Previously cleaned (update)", d.previously_cleaned, "previously_cleaned.csv"),
        ("No peak list", d.no_peaks_list, "no_peaks_list.csv"),
        ("No SMILES, InChI or InChIKey", d.no_smiles_no_inchi_no_inchikey, "no_smiles_no_inchi_no_inchikey.csv"),
        ("No precursor m/z", d.no_precursor_mz, "no_precursor_mz.csv"),
        ("No or bad adduct", d.no_or_bad_adduct, "no_or_bad_adduct.csv"),
        ("Low entropy score", d.low_entropy_score, "low_entropy_score.csv"),
        ("Too few peaks", d.minimum_peaks_not_requiered, "minimum_peaks_not_requiered.csv"),
        ("All peaks above precursor m/z", d.all_peaks_above_precursor_mz, "all_peaks_above_precursor_mz.csv"),
        ("No peak in m/z range", d.no_peaks_in_mz_range, "no_peaks_in_mz_range.csv"),
        ("Too few high peaks", d.minimum_high_peaks_not_requiered, "minimum_high_peaks_not_requiered.csv"),
        ("Low resolution MS2", d.low_resolution_ms2, "low_resolution_ms2.csv"),
        ("MS2 chemical crash test", d.ms2_chemical_crash, "ms2_chemical_crash_test.csv"),
        ("Duplicates removed", d.duplicatas_removed, "duplicatas_removed.csv"),
    ];
    let total_deleted: usize = deletions.iter().map(|(_, n, _)| n).sum();
    let deletions_json: Vec<Value> = deletions.iter().map(|(r, n, _)| json!({"reason": r, "count": n})).collect();

    // --- Paramètres ---
    let on_off = |key: &str| if get_bool(key) { "on" } else { "off" };
    let num = |x: f64| if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{x}") };
    let parameters = json!([
        {"name": "Normalize intensity", "state": on_off("normalize_intensity"), "value": ""},
        {"name": "Remove peaks above precursor m/z", "state": on_off("remove_peak_above_precursormz"), "value": ""},
        {"name": "Minimum number of peaks", "state": on_off("check_minimum_peak_requiered"),
         "value": num(get_f64("check_minimum_peak_requiered_n_peaks"))},
        {"name": "Maximum number of peaks kept", "state": on_off("reduce_peak_list"),
         "value": num(get_f64("reduce_peak_list_max_peaks"))},
        {"name": "Minimum normalized entropy", "state": on_off("remove_spectrum_under_entropy_score"),
         "value": num(get_f64("remove_spectrum_under_entropy_score_value"))},
        {"name": "m/z range", "state": on_off("keep_mz_in_range"),
         "value": format!("{} – {}", num(get_f64("keep_mz_in_range_from_mz")), num(get_f64("keep_mz_in_range_to_mz")))},
        {"name": "Minimum number of high peaks", "state": on_off("check_minimum_of_high_peaks_requiered"),
         "value": format!("{} peaks ≥ {} % of the base peak", num(get_f64("check_minimum_of_high_peaks_requiered_no_peaks")),
                          num(get_f64("check_minimum_of_high_peaks_requiered_intensity_percent")))},
        {"name": "De novo formulas", "state": on_off("calculate_de_novo"),
         "value": format!("{} ppm", num(get_f64("de_novo_ppm_tolerance")))},
        {"name": "Reset updates", "state": on_off("reset_updates"), "value": ""},
    ]);
    let formats: Vec<&str> = [("csv", "CSV"), ("msp", "MSP"), ("json", "JSON"), ("mzspeclib_json", "mzSpecLib")]
        .iter().filter(|(k, _)| get_bool(k)).map(|(_, n)| *n).collect();

    // --- Fichiers d'entrée : spectres gardés par fichier (nom du fichier d'entrée de chaque spectre) ---
    let mut kept_by_file: HashMap<&str, usize> = HashMap::new();
    for spec in libs.all_spectra() {
        *kept_by_file.entry(crate::globals_vars::input_filename(&spec.metadata)).or_insert(0) += 1;
    }
    let inputs: Vec<Value> = input_paths.iter().map(|p| {
        let path = Path::new(p);
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
        let database = input_db_names.get(p).map(|s| s.as_str()).filter(|s| !s.is_empty()).unwrap_or("Unknown");
        json!({
            "name": name,
            "folder": path.parent().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
            "format": input_format(p),
            "database": database,
            "size": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            "kept": kept_by_file.get(name.as_str()).copied().unwrap_or(0),
        })
    }).collect();

    // --- Ontologies, bases d'origine, instruments, remplissage des colonnes ---
    let mut classyfire = Hierarchy::default();
    let mut npclassifier = Hierarchy::default();
    let mut instruments = Hierarchy::default();
    let mut databases: BTreeMap<String, usize> = BTreeMap::new();
    let mut spec_to_dbs: HashMap<String, std::collections::BTreeSet<String>> = HashMap::new();
    let final_cols = ["FILENAME", "FILEHASH", "PREDICTED", "SPLASH", "SPECTRUMID", "RESOLUTION", "SYNON", "IONIZATION",
        "MSLEVEL", "FRAGMENTATIONMETHODE", "NAME", "PRECURSORMZ", "EXACTMASS", "AVERAGEMASS", "PRECURSORTYPE",
        "INSTRUMENTTYPE", "INSTRUMENT", "SMILES", "INCHI", "INCHIKEY", "COLLISIONENERGY", "FORMULA", "RT", "IONMODE",
        "COMMENT", "ENTROPY", "CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS", "NPCLASS_PATHWAY",
        "NPCLASS_SUPERCLASS", "NPCLASS_CLASS", "PEAKS_LIST"];
    let mut col_counts: HashMap<&str, usize> = HashMap::new();

    for spec in libs.all_spectra() {
        let m = &spec.metadata;
        classyfire.add("ClassyFire", &[class_value(m.get("CLASSYFIRE_SUPERCLASS")), class_value(m.get("CLASSYFIRE_CLASS")),
                                       class_value(m.get("CLASSYFIRE_SUBCLASS"))]);
        npclassifier.add("NPClassifier", &[class_value(m.get("NPCLASS_PATHWAY")), class_value(m.get("NPCLASS_SUPERCLASS")),
                                           class_value(m.get("NPCLASS_CLASS"))]);
        let tree = |k: &str| capitalize_words(m.get(k).map(|s| s.as_str()).unwrap_or("NOT FOUND"));
        instruments.add("Instrument", &[tree("TREE_MARQUE"), tree("TREE_MODELE"), tree("TREE_INSTYPE")]);

        // Bases de tous les doublons retirés (clé interne posée par le dédoublonnage), sinon la base du spectre
        let own = class_value(m.get("DATABASE_NAME"));
        let names: std::collections::BTreeSet<String> = match m.get(crate::globals_vars::ALL_DATABASE_NAMES_KEY) {
            Some(list) if list != "NOT FOUND" => list.split(crate::globals_vars::INTERNAL_LIST_SEP)
                .map(|n| { let n = n.trim(); if crate::globals_vars::EMPTY_PATTERN.is_match(n) { "Unknown".to_string() } else { n.to_string() } })
                .collect(),
            _ => std::iter::once(own).collect(),
        };
        for n in &names { *databases.entry(n.clone()).or_insert(0) += 1; }
        let id = format!("{}_{}", m.get("SPLASH").map(|s| s.as_str()).unwrap_or(""), m.get("INCHIKEY").map(|s| s.as_str()).unwrap_or(""));
        spec_to_dbs.entry(id).or_default().extend(names);

        for &col in &final_cols {
            let filled = if col == "PEAKS_LIST" { !spec.peaks.is_empty() } else {
                m.get(col).map(|v| {
                    let s = v.trim();
                    !s.is_empty() && !s.eq_ignore_ascii_case("nan") && !s.eq_ignore_ascii_case("not found") && !s.eq_ignore_ascii_case("unknown")
                }).unwrap_or(false)
            };
            if filled { *col_counts.entry(col).or_insert(0) += 1; }
        }
    }

    // Intersections des bases (UpSet) : combinaisons de bases d'un même spectre (SPLASH + InChIKey)
    let mut combinations: HashMap<Vec<String>, usize> = HashMap::new();
    for (_, set) in spec_to_dbs {
        *combinations.entry(set.into_iter().collect()).or_insert(0) += 1;
    }
    let mut upset: Vec<(Vec<String>, usize)> = combinations.into_iter().collect();
    upset.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let upset_json: Vec<Value> = upset.into_iter().map(|(sets, count)| json!({"sets": sets, "count": count})).collect();

    let columns: Vec<Value> = final_cols.iter().map(|c| json!({"column": c, "count": col_counts.get(c).copied().unwrap_or(0)})).collect();

    json!({
        "meta": meta,
        "inputs": inputs,
        "formats": formats,
        "parameters": parameters,
        "totals": {"input": total_kept + total_deleted, "deleted": total_deleted, "kept": total_kept, "inchikeys": total_unique},
        "deletions": deletions_json,
        "libraries": libraries,
        "groups": groups,
        "classyfire": classyfire.to_json(),
        "npclassifier": npclassifier.to_json(),
        "instruments": instruments.to_json(),
        "databases": databases,
        "upset": upset_json,
        "columns": columns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mots_en_majuscule_initiale() {
        assert_eq!(capitalize_words("lipids and lipid-like molecules"), "Lipids And Lipid-like Molecules");
        assert_eq!(capitalize_words("  deux   espaces "), "Deux Espaces");
        assert_eq!(capitalize_words("not found"), "NOT FOUND");
        assert_eq!(capitalize_words("Unknown"), "NOT FOUND");
        assert_eq!(capitalize_words("éther"), "Éther");
    }

    fn spectre(inchikey: &str, db: &str, cf: &str, file: &str) -> Spectrum {
        let mut s = Spectrum::default();
        for (k, v) in [("INCHIKEY", inchikey), ("DATABASE_NAME", db), ("CLASSYFIRE_SUPERCLASS", cf), ("SPLASH", "s"),
                       ("FILENAME", file), ("NAME", "x")] {
            s.metadata.insert(k.to_string(), v.to_string());
        }
        s.peaks = vec![(100.0, 1.0)];
        s
    }

    #[test]
    fn donnees_du_rapport() {
        let mut libs = crate::splitter::Libraries::default();
        libs.pos_lc = vec![spectre("AAA", "MoNA", "Lipids", "a.msp"), spectre("BBB", "GNPS", "NOT FOUND", "b.mgf")];
        libs.neg_lc = vec![spectre("AAA", "MoNA", "", "a.msp")];
        libs.pos_di = vec![spectre("CCC", "", "Benzenoids", "a.msp")];
        let d = crate::deletion_report::DeletionReport { duplicatas_removed: 2, low_entropy_score: 5, ..Default::default() };
        let params: HashMap<String, f64> = [("csv".to_string(), 1.0), ("keep_mz_in_range".to_string(), 1.0),
            ("keep_mz_in_range_from_mz".to_string(), 50.0), ("keep_mz_in_range_to_mz".to_string(), 2000.0)].into_iter().collect();
        let inputs = vec!["/d/a.msp".to_string(), "/d/b.mgf".to_string()];
        let names: HashMap<String, String> = [("/d/a.msp".to_string(), "MoNA".to_string())].into_iter().collect();
        let data = report_data("/sortie", "25_11_2026__15_14_36", "00:04:00", &params, &inputs, &names, &d, &libs);

        assert_eq!(data["meta"]["date"], "25-11-2026 15:14:36");
        assert_eq!(data["totals"], json!({"input": 11, "deleted": 7, "kept": 4, "inchikeys": 3}));
        // InChIKey différentes : AAA (LC POS et NEG) compte une fois pour LC
        assert_eq!(data["groups"]["LC"], 2);
        assert_eq!(data["groups"]["LC|POS"], 2);
        assert_eq!(data["groups"]["DI"], 1);
        assert_eq!(data["inputs"][0]["database"], "MoNA");
        assert_eq!(data["inputs"][1]["database"], "Unknown");
        assert_eq!(data["inputs"][0]["kept"], 3);
        assert_eq!(data["formats"], json!(["CSV"]));
        // Base vide et classe « NOT FOUND » : « Unknown »
        assert_eq!(data["databases"], json!({"GNPS": 1, "MoNA": 2, "Unknown": 1}));
        let labels: Vec<&str> = data["classyfire"]["labels"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(labels.contains(&"Unknown") && !labels.contains(&"NOT FOUND"));
        let reasons: Vec<&str> = data["deletions"].as_array().unwrap().iter().map(|v| v["reason"].as_str().unwrap()).collect();
        assert!(reasons.contains(&"Duplicates removed"));
    }

    #[test]
    fn texte_hors_balise_script() {
        // Un texte contenant « </script> » (ici un nom de base) ne doit pas fermer la balise des données
        let libs = crate::splitter::Libraries::default();
        let d = crate::deletion_report::DeletionReport::default();
        let names: HashMap<String, String> = [("/x/a.msp".to_string(), "</script><b>".to_string())].into_iter().collect();
        let html = build_report_html("/s", "d", "t", &HashMap::new(), &["/x/a.msp".to_string()], &names, &d, &libs);
        assert!(!html.contains("</script><b>"));
        assert!(html.contains("<\\/script><b>"));
    }
}
