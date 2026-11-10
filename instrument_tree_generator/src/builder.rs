//! Génération de l'arbre des instruments (`instruments_tree.json`) à partir de `Instrument_tree.xlsx`.
//!
//! Remplace `Generate_and_update_instrument_tree.py` (pandas). Même logique, ligne par ligne :
//! 1. chaque feuille de l'Excel est lue (colonnes MARQUES, MODELS, SPECTRUM_TYPE, INSTRUMENT_TYPE, IONISATION,
//!    RESOLUTION), valeurs nettoyées des espaces ;
//! 2. pour chaque ligne, toutes les variantes où des colonnes valent « NOT FOUND » (2^6 = 64 variantes),
//!    sauf celles où marque, modèle, LC/GC, analyseur et ionisation sont tous inconnus ;
//! 3. un analyseur inconnu est complété par l'analyseur le plus fréquent du modèle, un LC/GC inconnu par le
//!    LC/GC le plus fréquent du modèle (avec cette ionisation si elle est connue), une marque inconnue par la
//!    marque de la feuille (si le modèle est connu) ;
//! 4. la solution « Marque Modèle, TYPE, RÉSOLUTION » est écrite avec les valeurs de l'Excel ;
//! 5. les clés de l'arbre sont écrites comme le texte dans lequel FragHub les cherche (`search_text.rs`) ;
//! 6. dans chaque marque, les modèles sont rangés du plus long au plus court, toutes feuilles confondues ;
//! 7. au niveau résolution, une seule solution : HIGH ou LOW si toutes les lignes concernées sont d'accord,
//!    sinon « NOT FOUND » (la résolution n'est alors pas complétée).
//!
//! Différences voulues avec le script Python :
//! - le tri est stable : deux modèles de même longueur gardent l'ordre de l'Excel (avant, l'ordre dépendait
//!   du tri de pandas et pouvait changer d'une machine à l'autre ; or la recherche prend la première clé trouvée) ;
//! - le `strip` de la ligne 276 portait sur `df` au lieu de `df_filtered` : les modèles transformés
//!   (" qexactive ") gardaient leurs espaces au moment du tri ; ici ils sont retirés avant le tri ;
//! - plus de dépendance à la version de pandas (avec pandas 3, le script produisait un arbre faux sans erreur) ;
//! - clés écrites avec le nettoyage de la recherche : avant, 11 modèles ne pouvaient jamais être trouvés
//!   (« 6600  qqq » et « 5600  qqq » pour les TripleTOF, « 5500+ », « 6500+ », « ptrtof … », « jms hx/hx 110a ») ;
//! - modèle connu avec une ionisation : le LC/GC du modèle est complété aussi (avant, « ESI-QTOF » avec l'ionisation
//!   mais « LC-QTOF » sans elle : plus d'information donnait moins de résultat) ;
//! - modèles rangés par longueur sur tout l'arbre, et pas feuille par feuille : sans marque dans le texte,
//!   « Esquire 3000 » (Bruker) était pris pour le « 3000 » d'AB SCIEX, la feuille AB SCIEX passant avant ;
//! - résolution : FragHub prend HIGH dès qu'il existe, même quand LOW existe aussi au même endroit ; un « EI »,
//!   un « GC », un « QQQ » ou une marque seule recevaient donc HIGH. Quand le modèle est connu, la résolution
//!   est toujours unique dans l'Excel et elle est gardée. Les solutions « NOT FOUND » qui ne pouvaient jamais
//!   être choisies (la moitié de l'arbre) ne sont plus écrites.

use calamine::{open_workbook, Data, Reader, Xlsx};
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

/// Pour chaque (modèle, ionisation) connus : LC/GC le plus fréquent (égalité : ordre alphabétique).
/// Ex. micrOTOF-Q : LC avec ESI, GC avec APCI ; JMS DX-300 : LC avec FAB, GC avec CI.
fn spectrum_type_by_model_and_ionisation(rows: &[Row]) -> HashMap<(String, String), String> {
    let mut counts: HashMap<(&str, &str), HashMap<&str, usize>> = HashMap::new();
    for r in rows {
        if r[MODELE] != NF && r[IONI] != NF && r[SPECTYPE] != NF {
            *counts.entry((r[MODELE].as_str(), r[IONI].as_str())).or_default().entry(r[SPECTYPE].as_str()).or_default() += 1;
        }
    }
    counts.iter().filter_map(|((m, i), c)| most_frequent(c, |a, b| a < b)
        .map(|v| ((m.to_string(), i.to_string()), v.to_string()))).collect()
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
        // Modèle connu : l'analyseur et le LC/GC manquants sont complétés depuis le modèle
        // (avant, le LC/GC n'était pas complété quand l'ionisation était connue)
        let analyzer = if kt { r[INSTYPE].as_str() } else { it2 };
        match (ks, ki) {
            (true, true) => format!("{}-{}-{}", r[SPECTYPE], r[IONI], analyzer),
            (true, false) => format!("{}-{}", r[SPECTYPE], analyzer),
            (false, true) if st2 != NF => format!("{}-{}-{}", st2, r[IONI], analyzer),
            (false, true) => format!("{}-{}", r[IONI], analyzer),
            (false, false) => format!("{}-{}", st2, analyzer),
        }
    } else {
        let parts: Vec<&str> = [SPECTYPE, IONI, INSTYPE].iter().filter(|&&i| known(i)).map(|&i| r[i].as_str()).collect();
        if parts.is_empty() { NF.to_string() } else { parts.join("-") }
    };
    format!("{}, {}, {}", name, typ, r[RESOLUTION])
}

/// Clé d'un niveau de l'arbre : écrite comme le texte de recherche de FragHub.
fn make_key(level: usize, value: &str) -> String {
    let v = if level == MARQUE {
        match value { "AB SCIEX" => "SCIEX", "Thermo Fisher Scientific" => "Thermo", _ => value }
    } else { value };
    crate::search_text::tree_key(v)
}

/// Feuilles de l'arbre, dans l'ordre de l'Excel (feuille par feuille).
pub fn build_leaves(xlsx: &Path) -> Result<Vec<Leaf>, String> {
    let mut leaves = Vec::new();
    for (_name, rows) in read_sheets(xlsx)? {
        let expanded = expand(&rows);
        let it_by_model = instrument_type_by_model(&expanded);
        let st_by_model = spectrum_type_by_model(&expanded);
        let st_by_model_ioni = spectrum_type_by_model_and_ionisation(&expanded);
        let sheet_brand = expanded.iter().map(|r| r[MARQUE].as_str()).find(|b| *b != NF).unwrap_or(NF).to_string();
        let mut sheet: Vec<Leaf> = expanded.iter().map(|r| {
            let it2 = if r[INSTYPE] == NF { it_by_model.get(&r[MODELE]).map(|s| s.as_str()).unwrap_or(NF) } else { r[INSTYPE].as_str() };
            // LC/GC du modèle : avec l'ionisation connue si l'Excel a cette combinaison, sinon le plus fréquent
            let st2 = if r[MODELE] != NF && r[SPECTYPE] == NF {
                st_by_model_ioni.get(&(r[MODELE].clone(), r[IONI].clone())).or_else(|| st_by_model.get(&r[MODELE]))
                    .map(|s| s.as_str()).unwrap_or(r[SPECTYPE].as_str())
            } else { r[SPECTYPE].as_str() };
            Leaf {
                solution: format_solution(r, it2, st2, &sheet_brand),
                keys: std::array::from_fn(|k| make_key(k, &r[k])),
            }
        }).collect();
        leaves.append(&mut sheet);
    }
    Ok(leaves)
}

/// Arbre JSON : 6 niveaux de clés, puis {"SOLUTION": ...}. Un même chemin garde la dernière solution.
pub fn build_tree(leaves: &[Leaf]) -> Value {
    let mut root = Map::new();
    // Nombre de lignes de l'Excel derrière chaque chemin (chaque ligne donne au plus une variante par chemin)
    let mut rows_per_path: HashMap<&[String], usize> = HashMap::new();
    for leaf in leaves { *rows_per_path.entry(&leaf.keys[..]).or_default() += 1; }
    for leaf in leaves {
        let mut node = &mut root;
        for key in &leaf.keys {
            node = node.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut().expect("nœud de l'arbre");
        }
        node.insert("SOLUTION".to_string(), Value::String(leaf.solution.clone()));
    }
    for brand in root.values_mut() {
        let models = brand.as_object_mut().expect("marque");
        sort_models(models);
    }
    let mut path = Vec::new();
    for (brand, models) in root.iter_mut() {
        path.push(brand.clone());
        for (model, node) in models.as_object_mut().expect("marque").iter_mut() {
            path.push(model.clone());
            settle_resolution(node, &mut path, &rows_per_path);
            path.pop();
        }
        path.pop();
    }
    Value::Object(root)
}

/// Range les modèles du plus long au plus court (tri stable, « not found » en dernier).
/// La recherche prend la première clé trouvée : un modèle contenu dans un autre (« 3000 » dans « esquire 3000 »)
/// doit passer après lui, même s'il vient d'une autre feuille de l'Excel.
fn sort_models(models: &mut Map<String, Value>) {
    let mut entries: Vec<(String, Value)> = std::mem::take(models).into_iter().collect();
    entries.sort_by_key(|(k, _)| (k == "not found", std::cmp::Reverse(k.chars().count())));
    models.extend(entries);
}

/// Niveau résolution : une seule solution, celle que FragHub prendra.
/// - HIGH ou LOW si une seule des deux existe (toujours le cas quand le modèle est connu) ;
/// - les deux existent et l'analyseur est connu : la résolution de la majorité des lignes de l'Excel derrière ce
///   chemin (QTOF : 172 HIGH, 2 LOW -> HIGH ; QQQ : 6 HIGH, 17 LOW -> LOW) ; égalité : pas de résolution ;
/// - les deux existent sans analyseur (« ESI », « LC », « GC », une marque seule) : pas de résolution.
/// Avant, FragHub prenait HIGH dès qu'il existait. « Pas de résolution » = la solution « NOT FOUND », que FragHub
/// n'écrit pas ; elle est retirée quand HIGH ou LOW est gardé (FragHub ne la choisit jamais dans ce cas).
fn settle_resolution(node: &mut Value, path: &mut Vec<String>, rows_per_path: &HashMap<&[String], usize>) {
    let Some(map) = node.as_object_mut() else { return };
    if path.len() < RESOLUTION {
        for (k, child) in map.iter_mut() {
            path.push(k.clone());
            settle_resolution(child, path, rows_per_path);
            path.pop();
        }
        return;
    }
    let analyzer_known = path[INSTYPE] != "not found";
    let mut rows = |res: &str| -> usize {
        path.push(res.to_string());
        let n = rows_per_path.get(&path[..]).copied().unwrap_or(0);
        path.pop();
        n
    };
    let keep = match (map.contains_key("high"), map.contains_key("low")) {
        (true, false) => "high",
        (false, true) => "low",
        (true, true) if analyzer_known => {
            let (h, l) = (rows("high"), rows("low"));
            if h > l { "high" } else if l > h { "low" } else { "not found" }
        }
        _ => "not found",
    };
    map.retain(|k, _| k == keep);
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

/// Tableau des solutions (GRAPH_CONSTRUCT_SOLUTIONS.xlsx) : une ligne par solution de l'arbre, dans l'ordre de l'arbre.
fn write_solutions(path: &Path, tree: &Value) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};
    fn walk<'a>(v: &'a Value, path: &mut Vec<&'a str>, out: &mut Vec<(Vec<&'a str>, &'a str)>) {
        if let Some(map) = v.as_object() {
            for (k, child) in map {
                if k == "SOLUTION" { out.push((path.clone(), child.as_str().unwrap_or(""))); continue; }
                path.push(k);
                walk(child, path, out);
                path.pop();
            }
        }
    }
    let mut rows = Vec::new();
    walk(tree, &mut Vec::new(), &mut rows);
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    let bold = Format::new().set_bold();
    let err = |e: rust_xlsxwriter::XlsxError| format!("{} : {}", path.display(), e);
    for (c, name) in COLUMNS.iter().chain(std::iter::once(&"SOLUTION")).enumerate() {
        sheet.write_string_with_format(0, c as u16, *name, &bold).map_err(err)?;
    }
    for (i, (keys, solution)) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        for (c, key) in keys.iter().enumerate() {
            sheet.write_string(row, c as u16, *key).map_err(err)?;
        }
        sheet.write_string(row, COLUMNS.len() as u16, *solution).map_err(err)?;
    }
    workbook.save(path).map_err(err)
}

/// Génère l'arbre : lit l'Excel, écrit le JSON (et le tableau des solutions si demandé).
/// Retourne le nombre de solutions de l'arbre.
pub fn generate(xlsx: &Path, json: &Path, solutions: Option<&Path>) -> Result<usize, String> {
    let leaves = build_leaves(xlsx)?;
    let tree = build_tree(&leaves);
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut text = String::with_capacity(2 << 20);
    to_python_json(&tree, 0, nl, &mut text);
    std::fs::write(json, text).map_err(|e| format!("{} : {}", json.display(), e))?;
    if let Some(p) = solutions { write_solutions(p, &tree)?; }
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
    fn solution_ecrite_avec_ce_qui_est_connu() {
        let r = row(["Thermo Fisher Scientific", "Q-Exactive", "LC", "QFT", "ESI", "HIGH"]);
        assert_eq!(format_solution(&r, "QFT", "LC", "Thermo Fisher Scientific"), "Thermo Fisher Scientific Q-Exactive, LC-ESI-QFT, HIGH");
        // Marque inconnue : celle de la feuille ; analyseur inconnu : celui du modèle
        let r = row([NF, "Q-Exactive", "LC", NF, "ESI", "HIGH"]);
        assert_eq!(format_solution(&r, "ITFT", "LC", "Thermo"), "Thermo Q-Exactive, LC-ESI-ITFT, HIGH");
        // Modèle connu, LC/GC et ionisation inconnus : LC/GC du modèle
        let r = row(["Agilent", "6530", NF, "QTOF", NF, NF]);
        assert_eq!(format_solution(&r, "QTOF", "LC", "Agilent"), "Agilent 6530, LC-QTOF, NOT FOUND");
        // Modèle connu, LC/GC inconnu, ionisation connue : LC/GC du modèle aussi (avant : « ESI-QTOF »)
        let r = row(["Agilent", "6530", NF, "QTOF", "ESI", "HIGH"]);
        assert_eq!(format_solution(&r, "QTOF", "LC", "Agilent"), "Agilent 6530, LC-ESI-QTOF, HIGH");
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
    fn excel_lu_et_arbre_coherent() {
        let dir = std::env::temp_dir().join(format!("fraghub_arbre_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let xlsx = dir.join("Instrument_tree.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sheets: [(&str, Vec<[&str; 6]>); 2] = [
            ("AB SCIEX", vec![["AB SCIEX", "6600 TripleTOF", "LC", "QTOF", "ESI", "HIGH"], ["AB SCIEX ", "5500+", "LC", "QQQ", "ESI", "LOW"],
                              ["AB SCIEX", "3000", "LC", "QQQ", "ESI", "HIGH"]]),
            ("Bruker", vec![["Bruker", "Esquire 3000", "LC", "QIT", "ESI", "HIGH"], ["Bruker", "micrOTOF-Q", "LC", "QTOF", "ESI", "HIGH"],
                            ["Bruker", "micrOTOF-Q", "GC", "QTOF", "APCI", "HIGH"]]),
        ];
        for (name, rows) in &sheets {
            let ws = wb.add_worksheet().set_name(*name).unwrap();
            for (r, row) in std::iter::once(&COLUMNS).chain(rows.iter()).enumerate() {
                for (c, v) in row.iter().enumerate() { ws.write_string(r as u32, c as u16, *v).unwrap(); }
            }
        }
        wb.save(&xlsx).unwrap();
        let tree = build_tree(&build_leaves(&xlsx).unwrap());
        std::fs::remove_dir_all(&dir).ok();
        let sol = |p: [&str; 6]| tree[p[0]][p[1]][p[2]][p[3]][p[4]][p[5]]["SOLUTION"].as_str().map(|s| s.to_string());

        // Clés écrites comme le texte de recherche
        assert_eq!(sol(["sciex", "6600 qtof", "lc", "qtof", "esi", "high"]).unwrap(), "AB SCIEX 6600 TripleTOF, LC-ESI-QTOF, HIGH");
        assert_eq!(sol(["sciex", "5500", "lc", "qqq", "esi", "low"]).unwrap(), "AB SCIEX 5500+, LC-ESI-QQQ, LOW");
        // Marque absente : marque de la feuille ; LC/GC absent : celui du modèle, aussi quand l'ionisation est connue
        assert_eq!(sol(["not found", "5500", "not found", "not found", "esi", "low"]).unwrap(), "AB SCIEX 5500+, LC-ESI-QQQ, LOW");
        // ... et celui du modèle AVEC cette ionisation
        assert_eq!(sol(["bruker", "microtof q", "not found", "qtof", "apci", "high"]).unwrap(), "Bruker micrOTOF-Q, GC-APCI-QTOF, HIGH");
        assert_eq!(sol(["bruker", "microtof q", "not found", "qtof", "esi", "high"]).unwrap(), "Bruker micrOTOF-Q, LC-ESI-QTOF, HIGH");
        // Résolution : une seule solution. HIGH et LOW au même endroit sans analyseur -> pas de résolution
        let generic = tree["not found"]["not found"]["lc"]["not found"]["esi"].as_object().unwrap();
        assert_eq!(generic.keys().collect::<Vec<_>>(), ["not found"]);
        assert_eq!(generic["not found"]["SOLUTION"], "NOT FOUND, LC-ESI, NOT FOUND");
        // ... avec l'analyseur : la majorité (QQQ : 3000 HIGH, 5500+ LOW -> égalité ; QTOF : seulement HIGH)
        let qqq = tree["not found"]["not found"]["lc"]["qqq"]["esi"].as_object().unwrap();
        assert_eq!(qqq.keys().collect::<Vec<_>>(), ["not found"]);
        let qtof = tree["not found"]["not found"]["not found"]["qtof"]["not found"].as_object().unwrap();
        assert_eq!(qtof.keys().collect::<Vec<_>>(), ["high"]);
        assert!(sol(["sciex", "5500", "lc", "qqq", "esi", "not found"]).is_none());
        // Modèles du plus long au plus court sur tout l'arbre : « esquire 3000 » (Bruker) avant « 3000 » (AB SCIEX)
        let models: Vec<&String> = tree["not found"].as_object().unwrap().keys().collect();
        let pos = |k: &str| models.iter().position(|m| *m == k).unwrap();
        assert!(pos("esquire 3000") < pos("3000"), "{models:?}");
        assert_eq!(models.last().unwrap().as_str(), "not found");
    }
}
