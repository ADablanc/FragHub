use pyo3::prelude::*;
use std::collections::HashMap;
use std::fs;
use csv::ReaderBuilder;
use std::path::Path;
use crate::global_state::STATE;
use polars::prelude::*;

fn read_parquet_to_dict_of_dicts(filepath: &str, key_col: &str) -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();
    
    if let Ok(mut file) = fs::File::open(filepath) {
        if let Ok(df) = ParquetReader::new(&mut file).finish() {
            let headers: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
            let n_rows = df.height();
            let columns = df.get_columns();
            
            for i in 0..n_rows {
                let mut row_dict = HashMap::new();
                let mut key_val = String::new();
                
                for (col_idx, header) in headers.iter().enumerate() {
                    if let Ok(val) = columns[col_idx].get(i) {
                        let val_clean = match val {
                            AnyValue::String(s) => s.to_string(),
                            AnyValue::StringOwned(s) => s.to_string(),
                            AnyValue::Null => "".to_string(),
                            _ => val.to_string(),
                        };
                        if header == key_col {
                            key_val = val_clean.clone();
                        }
                        row_dict.insert(header.clone(), val_clean);
                    }
                }
                
                if !key_val.is_empty() {
                    map.insert(key_val, row_dict);
                }
            }
        }
    }
    map
}

/// Point d'entrée pour le chargement des bases de données depuis Python.
///
/// Pour un développeur Python : Cette fonction est appelée au démarrage de l'app Electron/Python.
/// Elle peuple le dictionnaire `STATE` global (le cache RAM).
#[pyfunction]
pub fn load_internal_databases(_py: Python, base_dir: &str) -> PyResult<()> {
    let base_path = Path::new(base_dir);

    // 1. Pubchem (Désormais fusionné dans ontologies_datas, on le laisse vide pour économiser la RAM)
    let pubchem_datas = HashMap::new();

    // 2. Ontologies (Contient désormais TOUTES les données PubChem + NPClassifier + ClassyFire au format Parquet)
    let ontologies_parquet_path = base_path.join("datas").join("internal_databases.parquet");
    let mut ontologies_datas = HashMap::new();
    
    if ontologies_parquet_path.is_dir() {
        if let Ok(entries) = fs::read_dir(&ontologies_parquet_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                    let map = read_parquet_to_dict_of_dicts(&path.to_string_lossy(), "INCHIKEY");
                    ontologies_datas.extend(map);
                }
            }
        }
    } else {
        ontologies_datas = read_parquet_to_dict_of_dicts(&ontologies_parquet_path.to_string_lossy(), "INCHIKEY");
    }

    // 3. Adducts
    let adduct_file_path = base_path.join("datas").join("adduct_to_convert.csv");
    let mut adduct_dict_pos = HashMap::new();
    let mut adduct_massdiff_dict_pos = HashMap::new();
    let mut adduct_dict_neg = HashMap::new();
    let mut adduct_massdiff_dict_neg = HashMap::new();

    if let Ok(mut rdr) = ReaderBuilder::new().delimiter(b';').from_path(&adduct_file_path) {
        if let Ok(headers) = rdr.headers() {
            let headers = headers.clone();
            let mut idx_known = 0; let mut idx_default = 1; let mut idx_massdiff = 2; let mut idx_ionmode = 3;
            for (i, h) in headers.iter().enumerate() {
                match h {
                    "known_adduct" => idx_known = i,
                    "fraghub_default" => idx_default = i,
                    "massdiff" => idx_massdiff = i,
                    "ionmode" => idx_ionmode = i,
                    _ => {}
                }
            }
            for result in rdr.records() {
                if let Ok(record) = result {
                    let known = record.get(idx_known).unwrap_or("").to_string();
                    let default = record.get(idx_default).unwrap_or("").to_string();
                    let massdiff_str = record.get(idx_massdiff).unwrap_or("0.0");
                    let massdiff: f64 = massdiff_str.parse().unwrap_or(0.0);
                    let ionmode = record.get(idx_ionmode).unwrap_or("");
                    let standard_known = crate::normalizer::normalize_adduct::standardize_adduct_key(&known);
                    if ionmode == "positive" {
                        // On insère uniquement la version standardisée.
                        // "or_insert" garantit que la PREMIÈRE ligne lue dans le CSV est conservée.
                        adduct_dict_pos.entry(standard_known).or_insert(default.clone());
                        adduct_massdiff_dict_pos.entry(default.clone()).or_insert(massdiff);
                    } else if ionmode == "negative" {
                        adduct_dict_neg.entry(standard_known).or_insert(default.clone());
                        adduct_massdiff_dict_neg.entry(default.clone()).or_insert(massdiff);
                    }
                }
            }
        }
    }

    // 4. Keys
    let keys_file_path = base_path.join("datas").join("key_to_convert.csv");
    let mut keys_dict = HashMap::new();
    if let Ok(mut rdr) = ReaderBuilder::new().delimiter(b';').from_path(&keys_file_path) {
        if let Ok(headers) = rdr.headers() {
            let headers = headers.clone();
            let mut idx_known = 0; let mut idx_default = 1;
            for (i, h) in headers.iter().enumerate() {
                match h {
                    "known_synonym" => idx_known = i,
                    "fraghub_default" => idx_default = i,
                    _ => {}
                }
            }
            for result in rdr.records() {
                if let Ok(record) = result {
                    let known = record.get(idx_known).unwrap_or("").to_string();
                    let default = record.get(idx_default).unwrap_or("").to_uppercase();
                    keys_dict.insert(known, default);
                }
            }
        }
    }

    // 5. Instrument tree
    let instrument_tree_path = base_path.join("datas").join("instruments_tree.json");
    let instrument_tree: serde_json::Value = if let Ok(content) = fs::read_to_string(&instrument_tree_path) {
        serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };

    // Update global state
    if let Ok(mut state) = STATE.write() {
        state.pubchem_datas = pubchem_datas;
        state.ontologies_datas = ontologies_datas;
        state.adduct_dict_pos = adduct_dict_pos;
        state.adduct_massdiff_dict_pos = adduct_massdiff_dict_pos;
        state.adduct_dict_neg = adduct_dict_neg;
        state.adduct_massdiff_dict_neg = adduct_massdiff_dict_neg;
        state.keys_dict = keys_dict;
        state.instrument_tree = instrument_tree;
    }

    Ok(())
}
