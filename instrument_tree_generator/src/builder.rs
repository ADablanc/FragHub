//! Génération de l'arbre des instruments (`instruments_tree.json`) à partir de `Instrument_tree.xlsx`.
//!
//! Remplace `Generate_and_update_instrument_tree.py` (pandas). Même logique, ligne par ligne :
//! 1. chaque feuille de l'Excel est lue (colonnes MARQUES, MODELS, SPECTRUM_TYPE, INSTRUMENT_TYPE, IONISATION,
//!    RESOLUTION), valeurs nettoyées des espaces ;
//! 2. pour chaque ligne, toutes les variantes où des colonnes valent « NOT FOUND » (2^6 = 64 variantes),
//!    sauf celles où marque, modèle, LC/GC, analyseur et ionisation sont tous inconnus ;
//! 3. un analyseur inconnu est complété par l'analyseur le plus fréquent du modèle, un LC/GC inconnu par le
//!    LC/GC le plus fréquent du modèle, une marque inconnue par la marque de la feuille (si le modèle est connu) ;
//! 4. la solution « Marque Modèle, TYPE, RÉSOLUTION » est écrite avec les valeurs de l'Excel ;
//! 5. les clés de l'arbre sont écrites comme le texte dans lequel FragHub les cherche (`search_text.rs`),
//!    les modèles triés du plus long au plus court dans chaque feuille.
//!
//! Différences voulues avec le script Python :
//! - le tri est stable : deux modèles de même longueur gardent l'ordre de l'Excel (avant, l'ordre dépendait
//!   du tri de pandas et pouvait changer d'une machine à l'autre ; or la recherche prend la première clé trouvée) ;
//! - le `strip` de la ligne 276 portait sur `df` au lieu de `df_filtered` : les modèles transformés
//!   (" qexactive ") gardaient leurs espaces au moment du tri ; ici ils sont retirés avant le tri ;
//! - plus de dépendance à la version de pandas (avec pandas 3, le script produisait un arbre faux sans erreur) ;
//! - clés écrites avec le nettoyage de la recherche : avant, 11 modèles ne pouvaient jamais être trouvés
//!   (« 6600  qqq » et « 5600  qqq » pour les TripleTOF, « 5500+ », « 6500+ », « ptrtof … », « jms hx/hx 110a »).

use calamine::{open_workbook, Data, Reader, Xlsx};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

const NF: &str = "NOT FOUND";
/// Colonnes de l'Excel, dans l'ordre des niveaux de l'arbre.
const COLUMNS: [&str; 6] = ["MARQUES", "MODELS", "SPECTRUM_TYPE", "INSTRUMENT_TYPE", "IONISATION", "RESOLUTION"];
const MARQUE: usize = 0;
const MODELE: usize = 1;
const SPECTYPE: usize = 2;
const INSTYPE: usize = 3;
const IONI: usize = 4;
const RESOLUTION: usize = 5;

type Row = [String; 6];

/// Fabrication des clés de l'arbre.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KeyStyle {
    /// Comme le script Python : minuscules et quelques remplacements sur les modèles.
    /// Gardé pour vérifier que le générateur Rust redonne l'arbre du script (test `excel_lu_et_cles_comme_la_recherche` ; vérifié aussi sur l'arbre complet : mêmes 10 277 solutions).
    #[allow(dead_code)]
    Legacy,
    /// Même nettoyage que le texte dans lequel on cherche (`normalize_instruments_and_resolution::tree_key`) :
    /// avant, certaines clés ne pouvaient jamais être trouvées (« 6600  qqq » pour TripleTOF, « ptrtof 1000 »,
    /// « 5500+ », « jms hx/hx 110a »).
    Search,
}

/// Une feuille de l'arbre : le chemin (6 clés) et sa solution.
pub struct Leaf {
    pub keys: Row,
    pub solution: String,
}

/// Texte d'une cellule, comme `astype(str)` de pandas (les nombres entiers sans « .0 »).
fn cell_text(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => format!("{}", f),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
        Data::Empty => "nan".to_string(),
        other => other.to_string(),
    }
}

/// Lit toutes les feuilles de l'Excel, dans l'ordre du classeur.
fn read_sheets(path: &Path) -> Result<Vec<(String, Vec<Row>)>, String> {
    let mut workbook: Xlsx<_> = open_workbook(path).map_err(|e| format!("{} : {}", path.display(), e))?;
    let mut sheets = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook.worksheet_range(&name).map_err(|e| format!("feuille « {} » : {}", name, e))?;
        let mut rows = range.rows();
        let header: Vec<String> = match rows.next() {
            Some(h) => h.iter().map(|c| cell_text(c).trim().to_string()).collect(),
            None => continue,
        };
        // Les colonnes doivent être dans l'ordre des niveaux (l'ordre des variantes en dépend)
        let index: Vec<usize> = COLUMNS.iter().map(|c| header.iter().position(|h| h == c)
            .ok_or_else(|| format!("feuille « {} » : colonne {} absente", name, c))).collect::<Result<_, _>>()?;
        if index.windows(2).any(|w| w[0] > w[1]) {
            return Err(format!("feuille « {} » : colonnes attendues dans l'ordre {}", name, COLUMNS.join(", ")));
        }
        let mut sheet_rows = Vec::new();
        for r in rows {
            if r.iter().all(|c| matches!(c, Data::Empty)) { continue; }
            let cell = |i: usize| r.get(i).map(cell_text).unwrap_or_else(|| "nan".to_string()).trim().to_string();
            sheet_rows.push(std::array::from_fn(|k| cell(index[k])));
        }
        sheets.push((name, sheet_rows));
    }
    Ok(sheets)
}

/// Sous-ensembles de {0..n-1} de taille r, dans l'ordre de `itertools.combinations`.
fn combinations(n: usize, r: usize) -> Vec<Vec<usize>> {
    if r > n { return Vec::new(); }
    let mut idx: Vec<usize> = (0..r).collect();
    let mut out = vec![idx.clone()];
    loop {
        // Indice le plus à droite qui peut encore avancer
        let Some(i) = (0..r).rev().find(|&i| idx[i] != i + n - r) else { return out; };
        idx[i] += 1;
        for j in i + 1..r { idx[j] = idx[j - 1] + 1; }
        out.push(idx.clone());
    }
}

/// Variantes « NOT FOUND » d'une feuille, sans celles où les 5 premiers niveaux sont inconnus.
fn expand(rows: &[Row]) -> Vec<Row> {
    let mut out = Vec::new();
    for r in 0..=COLUMNS.len() {
        for subset in combinations(COLUMNS.len(), r) {
            for row in rows {
                let mut new_row = row.clone();
                for &c in &subset { new_row[c] = NF.to_string(); }
                if new_row[..RESOLUTION].iter().all(|v| v == NF) { continue; }
                out.push(new_row);
            }
        }
    }
    out
}

/// Valeur la plus fréquente ; en cas d'égalité, choix de `better`.
fn most_frequent<'a>(counts: &HashMap<&'a str, usize>, better: fn(&str, &str) -> bool) -> Option<&'a str> {
    let max = *counts.values().max()?;
    let mut best: Option<&str> = None;
    for (&v, &n) in counts {
        if n == max && best.map_or(true, |b| better(v, b)) { best = Some(v); }
    }
    best
}

/// Pour chaque modèle : analyseur le plus fréquent (pandas : `mode()`, puis le plus court, puis l'ordre alphabétique).
fn instrument_type_by_model(rows: &[Row]) -> HashMap<String, String> {
    let mut counts: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for r in rows {
        if r[INSTYPE] != NF {
            *counts.entry(r[MODELE].as_str()).or_default().entry(r[INSTYPE].as_str()).or_default() += 1;
        }
    }
    counts.iter().filter_map(|(m, c)| most_frequent(c, |a, b| (a.chars().count(), a) < (b.chars().count(), b))
        .map(|v| (m.to_string(), v.to_string()))).collect()
}

/// Pour chaque modèle connu : LC/GC le plus fréquent (pandas : `mode()[0]`, le premier dans l'ordre alphabétique).
fn spectrum_type_by_model(rows: &[Row]) -> HashMap<String, String> {
    let mut counts: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for r in rows {
        if r[MODELE] != NF && r[SPECTYPE] != NF {
            *counts.entry(r[MODELE].as_str()).or_default().entry(r[SPECTYPE].as_str()).or_default() += 1;
        }
    }
    counts.iter().filter_map(|(m, c)| most_frequent(c, |a, b| a < b).map(|v| (m.to_string(), v.to_string()))).collect()
}

/// Solution « Marque Modèle, TYPE, RÉSOLUTION » (fonction `format_solution` du script Python).
/// `it2` / `st2` : analyseur et LC/GC complétés depuis le modèle ; `sheet_brand` : marque de la feuille.
fn format_solution(r: &Row, it2: &str, st2: &str, sheet_brand: &str) -> String {
    let known = |i: usize| r[i] != NF;
    let (kb, km, ks, kt, ki) = (known(MARQUE), known(MODELE), known(SPECTYPE), known(INSTYPE), known(IONI));
    if !kb && !km && !ks && !kt && !ki {
        return "NOT FOUND, NOT FOUND, NOT FOUND".to_string();
    }
    let name = match (kb, km) {
        (true, true) => format!("{} {}", r[MARQUE], r[MODELE]),
        (true, false) => format!("{} instrument", r[MARQUE]),
        (false, true) => format!("{} {}", sheet_brand, r[MODELE]),
        (false, false) => NF.to_string(),
    };
    let typ = if km {
        // Modèle connu : l'analyseur manquant est complété depuis le modèle ; le LC/GC aussi, sauf si l'ionisation
        // est connue (le script écrit alors « ionisation-analyseur »)
        let analyzer = if kt { r[INSTYPE].as_str() } else { it2 };
        match (ks, ki) {
            (true, true) => format!("{}-{}-{}", r[SPECTYPE], r[IONI], analyzer),
            (true, false) => format!("{}-{}", r[SPECTYPE], analyzer),
            (false, true) => format!("{}-{}", r[IONI], analyzer),
            (false, false) => format!("{}-{}", st2, analyzer),
        }
    } else {
        let parts: Vec<&str> = [SPECTYPE, IONI, INSTYPE].iter().filter(|&&i| known(i)).map(|&i| r[i].as_str()).collect();
        if parts.is_empty() { NF.to_string() } else { parts.join("-") }
    };
    format!("{}, {}, {}", name, typ, r[RESOLUTION])
}

/// Clé d'un niveau de l'arbre.
fn make_key(level: usize, value: &str, style: KeyStyle) -> String {
    match style {
        KeyStyle::Legacy => {
            let mut v = value.to_string();
            if level == MARQUE {
                v = match v.as_str() { "AB SCIEX" => "SCIEX".to_string(), "Thermo Fisher Scientific" => "Thermo".to_string(), _ => v };
            }
            v = v.to_lowercase();
            if level == MODELE {
                static TRIPLE_TOF: Lazy<Regex> = Lazy::new(|| Regex::new(r"triple(-| )?tof").unwrap());
                static TRIPLE_QUAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"triple(-| )?quad").unwrap());
                v = v.replace("-tof", "tof").replace("q-", "q").replace("q exactive", " qexactive ").replace('-', " ");
                v = TRIPLE_TOF.replace_all(&v, " qqq ").into_owned();
                v = TRIPLE_QUAD.replace_all(&v, " qqq ").into_owned();
            }
            v.trim().to_string()
        }
        KeyStyle::Search => {
            let v = if level == MARQUE {
                match value { "AB SCIEX" => "SCIEX", "Thermo Fisher Scientific" => "Thermo", _ => value }
            } else { value };
            crate::search_text::tree_key(v)
        }
    }
}

/// Feuilles de l'arbre, dans l'ordre (feuilles de l'Excel, puis modèles du plus long au plus court).
pub fn build_leaves(xlsx: &Path, style: KeyStyle) -> Result<Vec<Leaf>, String> {
    let mut leaves = Vec::new();
    for (_name, rows) in read_sheets(xlsx)? {
        let expanded = expand(&rows);
        let it_by_model = instrument_type_by_model(&expanded);
        let st_by_model = spectrum_type_by_model(&expanded);
        let sheet_brand = expanded.iter().map(|r| r[MARQUE].as_str()).find(|b| *b != NF).unwrap_or(NF).to_string();
        let mut sheet: Vec<Leaf> = expanded.iter().map(|r| {
            let it2 = if r[INSTYPE] == NF { it_by_model.get(&r[MODELE]).map(|s| s.as_str()).unwrap_or(NF) } else { r[INSTYPE].as_str() };
            let st2 = if r[MODELE] != NF && r[SPECTYPE] == NF {
                st_by_model.get(&r[MODELE]).map(|s| s.as_str()).unwrap_or(r[SPECTYPE].as_str())
            } else { r[SPECTYPE].as_str() };
            Leaf {
                solution: format_solution(r, it2, st2, &sheet_brand),
                keys: std::array::from_fn(|k| make_key(k, &r[k], style)),
            }
        }).collect();
        // Tri stable : les modèles les plus longs d'abord (la recherche prend la première clé trouvée)
        sheet.sort_by(|a, b| b.keys[MODELE].chars().count().cmp(&a.keys[MODELE].chars().count()));
        leaves.extend(sheet);
    }
    Ok(leaves)
}

/// Arbre JSON : 6 niveaux de clés, puis {"SOLUTION": ...}. Un même chemin garde la dernière solution.
pub fn build_tree(leaves: &[Leaf]) -> Value {
    let mut root = Map::new();
    for leaf in leaves {
        let mut node = &mut root;
        for key in &leaf.keys {
            node = node.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut().expect("nœud de l'arbre");
        }
        node.insert("SOLUTION".to_string(), Value::String(leaf.solution.clone()));
    }
    Value::Object(root)
}

/// Chaîne JSON comme `json.dump` de Python (caractères non ASCII écrits en \uXXXX).
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) { out.push_str(&format!("\\u{:04x}", u)); }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JSON indenté de 2 espaces, comme `json.dump(D, f, indent=2)`.
fn to_python_json(v: &Value, depth: usize, nl: &str, out: &mut String) {
    match v {
        Value::Object(map) if !map.is_empty() => {
            out.push('{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(nl);
                out.push_str(&"  ".repeat(depth + 1));
                push_json_string(out, k);
                out.push_str(": ");
                to_python_json(val, depth + 1, nl, out);
            }
            out.push_str(nl);
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
        Value::Object(_) => out.push_str("{}"),
        Value::String(s) => push_json_string(out, s),
        other => out.push_str(&other.to_string()),
    }
}

/// Tableau des solutions (GRAPH_CONSTRUCT_SOLUTIONS.xlsx) : une ligne par feuille de l'arbre.
fn write_solutions(path: &Path, leaves: &[Leaf]) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    let bold = Format::new().set_bold();
    let err = |e: rust_xlsxwriter::XlsxError| format!("{} : {}", path.display(), e);
    for (c, name) in COLUMNS.iter().chain(std::iter::once(&"SOLUTION")).enumerate() {
        sheet.write_string_with_format(0, c as u16, *name, &bold).map_err(err)?;
    }
    for (i, leaf) in leaves.iter().enumerate() {
        let row = (i + 1) as u32;
        for (c, key) in leaf.keys.iter().enumerate() {
            sheet.write_string(row, c as u16, key).map_err(err)?;
        }
        sheet.write_string(row, COLUMNS.len() as u16, &leaf.solution).map_err(err)?;
    }
    workbook.save(path).map_err(err)
}

/// Génère l'arbre : lit l'Excel, écrit le JSON (et le tableau des solutions si demandé).
/// Retourne le nombre de solutions de l'arbre.
pub fn generate(xlsx: &Path, json: &Path, solutions: Option<&Path>, style: KeyStyle) -> Result<usize, String> {
    let leaves = build_leaves(xlsx, style)?;
    let tree = build_tree(&leaves);
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut text = String::with_capacity(2 << 20);
    to_python_json(&tree, 0, nl, &mut text);
    std::fs::write(json, text).map_err(|e| format!("{} : {}", json.display(), e))?;
    if let Some(p) = solutions { write_solutions(p, &leaves)?; }
    Ok(count_solutions(&tree))
}

fn count_solutions(v: &Value) -> usize {
    match v.as_object() {
        Some(m) => m.iter().map(|(k, val)| if k == "SOLUTION" { 1 } else { count_solutions(val) }).sum(),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinaisons_dans_l_ordre_d_itertools() {
        assert_eq!(combinations(3, 0), vec![Vec::<usize>::new()]);
        assert_eq!(combinations(3, 2), vec![vec![0, 1], vec![0, 2], vec![1, 2]]);
        assert_eq!(combinations(4, 4), vec![vec![0, 1, 2, 3]]);
        assert_eq!((0..=6).map(|r| combinations(6, r).len()).sum::<usize>(), 64);
    }

    fn row(v: [&str; 6]) -> Row { std::array::from_fn(|i| v[i].to_string()) }

    #[test]
    fn solution_comme_le_script_python() {
        let r = row(["Thermo Fisher Scientific", "Q-Exactive", "LC", "QFT", "ESI", "HIGH"]);
        assert_eq!(format_solution(&r, "QFT", "LC", "Thermo Fisher Scientific"), "Thermo Fisher Scientific Q-Exactive, LC-ESI-QFT, HIGH");
        // Marque inconnue : celle de la feuille ; analyseur inconnu : celui du modèle
        let r = row([NF, "Q-Exactive", "LC", NF, "ESI", "HIGH"]);
        assert_eq!(format_solution(&r, "ITFT", "LC", "Thermo"), "Thermo Q-Exactive, LC-ESI-ITFT, HIGH");
        // Modèle connu, LC/GC et ionisation inconnus : LC/GC du modèle
        let r = row(["Agilent", "6530", NF, "QTOF", NF, NF]);
        assert_eq!(format_solution(&r, "QTOF", "LC", "Agilent"), "Agilent 6530, LC-QTOF, NOT FOUND");
        // Modèle inconnu : seulement ce qui est connu
        let r = row(["Hitachi", NF, NF, "B", NF, "LOW"]);
        assert_eq!(format_solution(&r, "B", NF, "Hitachi"), "Hitachi instrument, B, LOW");
        let r = row([NF, NF, NF, NF, NF, "LOW"]);
        assert_eq!(format_solution(&r, NF, NF, "Hitachi"), "NOT FOUND, NOT FOUND, NOT FOUND");
    }

    #[test]
    fn mode_du_modele() {
        let rows = vec![row(["A", "m", "LC", "QFT", "ESI", "HIGH"]), row(["A", "m", "GC", "ITFT", "ESI", "HIGH"]),
                        row(["A", "m", "LC", "FT", "ESI", "HIGH"])];
        // Égalité entre trois analyseurs : le plus court, puis l'ordre alphabétique
        assert_eq!(instrument_type_by_model(&rows)["m"], "FT");
        assert_eq!(spectrum_type_by_model(&rows)["m"], "LC");
    }

    #[test]
    fn json_comme_python() {
        let v: Value = serde_json::from_str(r#"{"a": {"SOLUTION": "x é"}, "b": {}}"#).unwrap();
        let mut s = String::new();
        to_python_json(&v, 0, "\n", &mut s);
        assert_eq!(s, "{\n  \"a\": {\n    \"SOLUTION\": \"x \\u00e9\"\n  },\n  \"b\": {}\n}");
    }

    #[test]
    fn excel_lu_et_cles_comme_la_recherche() {
        // Petit Excel : une feuille, deux modèles
        let dir = std::env::temp_dir().join(format!("fraghub_arbre_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let xlsx = dir.join("Instrument_tree.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet().set_name("AB SCIEX").unwrap();
        let rows = [COLUMNS, ["AB SCIEX", "6600 TripleTOF", "LC", "QTOF", "ESI", "HIGH"], ["AB SCIEX ", "5500+", "LC", "QQQ", "ESI", "LOW"]];
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() { ws.write_string(r as u32, c as u16, *v).unwrap(); }
        }
        wb.save(&xlsx).unwrap();

        let tree = build_tree(&build_leaves(&xlsx, KeyStyle::Search).unwrap());
        assert_eq!(tree["sciex"]["6600 qtof"]["lc"]["qtof"]["esi"]["high"]["SOLUTION"], "AB SCIEX 6600 TripleTOF, LC-ESI-QTOF, HIGH");
        assert_eq!(tree["sciex"]["5500"]["lc"]["qqq"]["esi"]["low"]["SOLUTION"], "AB SCIEX 5500+, LC-ESI-QQQ, LOW");
        // Marque absente : marque de la feuille ; LC/GC absent : celui du modèle
        assert_eq!(tree["not found"]["5500"]["not found"]["not found"]["not found"]["not found"]["SOLUTION"], "AB SCIEX 5500+, LC-QQQ, NOT FOUND");
        // Modèle le plus long d'abord
        assert_eq!(tree["sciex"].as_object().unwrap().keys().next().unwrap(), "6600 qtof");
        // 2 x 62 variantes (64, moins les 2 où tout est inconnu), dont 7 chemins communs aux deux lignes
        assert_eq!(count_solutions(&tree), 117);
        // Clés du script Python : « 6600  qqq » (double espace, jamais trouvée), « 5500+ »
        let old = build_tree(&build_leaves(&xlsx, KeyStyle::Legacy).unwrap());
        assert!(old["sciex"].get("6600  qqq").is_some() && old["sciex"].get("5500+").is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
}
