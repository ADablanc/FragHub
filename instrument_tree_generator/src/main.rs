//! Génère l'arbre des instruments de FragHub à partir de l'Excel.
//!
//! Depuis la racine du projet :
//!     cargo run --release --manifest-path instrument_tree_generator/Cargo.toml
//! Par défaut : lit `Instrument_tree.xlsx`, écrit `instruments_tree.json` et `GRAPH_CONSTRUCT_SOLUTIONS.xlsx`
//! à la racine du projet (dossier parent de cet outil). Autres fichiers :
//!     ... -- --xlsx autre.xlsx --json sortie.json --solutions solutions.xlsx
mod builder;
mod search_text;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Racine du projet : dossier parent de l'outil
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().map(PathBuf::from).unwrap_or_default();
    let mut xlsx = root.join("Instrument_tree.xlsx");
    let mut json = root.join("instruments_tree.json");
    let mut solutions = Some(root.join("GRAPH_CONSTRUCT_SOLUTIONS.xlsx"));

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().map(PathBuf::from);
        match a.as_str() {
            "--xlsx" => if let Some(v) = value() { xlsx = v },
            "--json" => if let Some(v) = value() { json = v },
            "--solutions" => solutions = value(),
            "--no-solutions" => solutions = None,
            _ => {
                eprintln!("Option inconnue : {a}\nOptions : --xlsx <fichier> --json <fichier> --solutions <fichier> --no-solutions");
                return ExitCode::FAILURE;
            }
        }
    }

    match builder::generate(&xlsx, &json, solutions.as_deref(), builder::KeyStyle::Search) {
        Ok(n) => {
            println!("{n} solutions écrites dans {}", json.display());
            if let Some(s) = &solutions { println!("Tableau des solutions : {}", s.display()); }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Erreur : {e}");
            ExitCode::FAILURE
        }
    }
}
