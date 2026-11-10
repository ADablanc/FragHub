// src/output_guard.rs
//! Protection du dossier de sortie pendant un run : si le run s'arrête avant la fin (bouton stop, Ctrl+C, erreur),
//! le dossier de sortie est remis exactement comme avant le run. Les fichiers déjà présents ne sont jamais abîmés.
//!
//! Avant d'écrire dans un fichier, chaque writer appelle `prepare(chemin, ajout)`. La première fois qu'un fichier est
//! vu dans le run, son état est noté :
//! - fichier nouveau : supprimé au retour arrière ;
//! - ajout à la suite (mode mise à jour) : sa taille et ses derniers octets sont notés ; au retour arrière il est
//!   recoupé à sa taille et ses derniers octets remis (le writer JSON réécrit la fin du fichier pour ajouter) ;
//! - remplacement : l'ancien fichier est mis de côté (renommé, sans copie), puis remis au retour arrière.
//! La remise à zéro des mises à jour (`reset_updates`) met de même de côté le contenu du dossier au lieu de le
//! supprimer. En fin de run réussi, `commit` supprime ce qui a été mis de côté.
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Suffixe d'un fichier remplacé, mis de côté pendant le run
const BACKUP_SUFFIX: &str = ".fraghub_backup";
/// Dossier où la remise à zéro des mises à jour met de côté l'ancien contenu du dossier de sortie
pub const PREVIOUS_DIR: &str = ".fraghub_previous_run";
/// Nombre de derniers octets gardés pour un fichier complété
const TAIL_LEN: u64 = 4096;

enum Change {
    Created,
    Appended { len: u64, tail: Vec<u8> },
    Replaced { backup: PathBuf },
}

struct Journal {
    entries: Vec<(PathBuf, Change)>,
    /// Contenu mis de côté par la remise à zéro : (dossier de sortie, dossier de mise de côté, noms déplacés)
    previous: Option<(PathBuf, PathBuf, Vec<std::ffi::OsString>)>,
}

static JOURNAL: Mutex<Option<Journal>> = Mutex::new(None);

/// Début d'un run : journal vide.
pub fn begin() {
    *JOURNAL.lock().unwrap() = Some(Journal { entries: Vec::new(), previous: None });
}

/// À appeler avant d'ouvrir `path` en écriture (`append` : ajout à la suite, sinon remplacement).
/// Sans journal ouvert (tests, outils), ne fait rien.
pub fn prepare(path: &Path, append: bool) -> std::io::Result<()> {
    let mut guard = JOURNAL.lock().unwrap();
    let Some(journal) = guard.as_mut() else { return Ok(()) };
    if journal.entries.iter().any(|(p, _)| p == path) {
        return Ok(()); // déjà noté dans ce run : son état d'avant le run est connu
    }
    let change = if !path.exists() {
        Change::Created
    } else if append {
        let len = fs::metadata(path)?.len();
        let start = len.saturating_sub(TAIL_LEN);
        let mut f = fs::File::open(path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::with_capacity((len - start) as usize);
        f.read_to_end(&mut tail)?;
        Change::Appended { len, tail }
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(BACKUP_SUFFIX);
        let backup = PathBuf::from(name);
        if backup.exists() {
            fs::remove_file(&backup)?; // reste d'un run interrompu de force
        }
        fs::rename(path, &backup)?;
        Change::Replaced { backup }
    };
    journal.entries.push((path.to_path_buf(), change));
    Ok(())
}

/// Même chose, appelée depuis Python (écriture du mzSpecLib, scripts/mzspeclib_converter.py).
#[pyo3::pyfunction]
pub fn guard_output_file(path: String, append: bool) -> pyo3::PyResult<()> {
    prepare(Path::new(&path), append)?;
    Ok(())
}

/// Remise à zéro des mises à jour : le contenu du dossier de sortie est mis de côté (déplacé dans
/// `PREVIOUS_DIR`), au lieu d'être supprimé tout de suite. Supprimé en fin de run réussi, remis sinon.
pub fn set_aside_output_directory(out: &Path) -> std::io::Result<()> {
    let mut guard = JOURNAL.lock().unwrap();
    let previous = out.join(PREVIOUS_DIR);
    if previous.exists() {
        fs::remove_dir_all(&previous)?; // reste d'un run interrompu de force
    }
    let mut moved = Vec::new();
    if out.is_dir() {
        let names: Vec<_> = fs::read_dir(out)?.filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        if !names.is_empty() {
            fs::create_dir_all(&previous)?;
        }
        for name in names {
            fs::rename(out.join(&name), previous.join(&name))?;
            moved.push(name);
        }
    }
    match guard.as_mut() {
        Some(journal) => journal.previous = Some((out.to_path_buf(), previous, moved)),
        // Sans journal (appel direct depuis Python) : comportement d'avant, suppression
        None => {
            if previous.exists() { fs::remove_dir_all(&previous)?; }
        }
    }
    Ok(())
}

/// Fin de run réussie : ce qui a été mis de côté est supprimé.
pub fn commit() {
    let Some(journal) = JOURNAL.lock().unwrap().take() else { return };
    for (_, change) in journal.entries {
        if let Change::Replaced { backup } = change {
            let _ = fs::remove_file(backup);
        }
    }
    if let Some((_, previous, _)) = journal.previous {
        let _ = fs::remove_dir_all(previous);
    }
}

/// Run arrêté ou en erreur : le dossier de sortie est remis comme avant le run.
/// Renvoie les fichiers qui n'ont pas pu être remis (normalement aucun).
pub fn rollback() -> Vec<String> {
    let Some(journal) = JOURNAL.lock().unwrap().take() else { return Vec::new() };
    let mut failed = Vec::new();
    for (path, change) in journal.entries.into_iter().rev() {
        let res = match change {
            Change::Created => if path.exists() { fs::remove_file(&path) } else { Ok(()) },
            Change::Appended { len, tail } => restore_tail(&path, len, &tail),
            Change::Replaced { backup } => {
                if path.exists() { let _ = fs::remove_file(&path); }
                fs::rename(&backup, &path)
            }
        };
        if let Err(e) = res {
            failed.push(format!("{} ({e})", path.display()));
        }
    }
    if let Some((out, previous, moved)) = journal.previous {
        // Tout ce que le run a créé est retiré, puis l'ancien contenu remis
        if let Ok(entries) = fs::read_dir(&out) {
            for e in entries.filter_map(|e| e.ok()) {
                if e.file_name() == PREVIOUS_DIR { continue; }
                let p = e.path();
                let _ = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            }
        }
        for name in moved {
            if let Err(e) = fs::rename(previous.join(&name), out.join(&name)) {
                failed.push(format!("{} ({e})", out.join(&name).display()));
            }
        }
        let _ = fs::remove_dir_all(&previous);
    }
    failed
}

fn restore_tail(path: &Path, len: u64, tail: &[u8]) -> std::io::Result<()> {
    let mut f = OpenOptions::new().write(true).open(path)?;
    f.set_len(len)?;
    f.seek(SeekFrom::Start(len - tail.len() as u64))?;
    f.write_all(tail)?;
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Les tests partagent le journal global : un seul test, en séquence
    #[test]
    fn retour_arriere_et_validation() {
        let d = std::env::temp_dir().join(format!("fraghub_guard_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let (a, b, c) = (d.join("ajout.json"), d.join("remplace.csv"), d.join("nouveau.msp"));
        fs::write(&a, b"[\n  {\"x\": 1}\n]").unwrap();
        fs::write(&b, b"ancien").unwrap();

        // Run arrêté : tout est remis
        begin();
        prepare(&a, true).unwrap();
        // le writer JSON réécrit la fin puis ajoute
        let mut f = OpenOptions::new().write(true).open(&a).unwrap();
        f.seek(SeekFrom::End(-2)).unwrap();
        f.write_all(b",\n  {\"x\": 2}\n]").unwrap();
        prepare(&b, false).unwrap();
        fs::write(&b, b"nouveau contenu").unwrap();
        prepare(&c, false).unwrap();
        fs::write(&c, b"cree").unwrap();
        prepare(&a, true).unwrap(); // deuxième fois : sans effet
        assert!(rollback().is_empty());
        assert_eq!(fs::read(&a).unwrap(), b"[\n  {\"x\": 1}\n]");
        assert_eq!(fs::read(&b).unwrap(), b"ancien");
        assert!(!c.exists());
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2, "aucun fichier de côté ne doit rester");

        // Run réussi : les nouvelles versions restent, rien n'est laissé de côté
        begin();
        prepare(&b, false).unwrap();
        fs::write(&b, b"nouveau").unwrap();
        commit();
        assert_eq!(fs::read(&b).unwrap(), b"nouveau");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2);

        // Remise à zéro des mises à jour puis arrêt : ancien contenu remis, contenu du run retiré
        let out = d.join("sortie");
        fs::create_dir_all(out.join("CSV/POS")).unwrap();
        fs::write(out.join("CSV/POS/POS_LC.csv"), b"ancien run").unwrap();
        fs::write(out.join("updates.json"), b"{}").unwrap();
        begin();
        set_aside_output_directory(&out).unwrap();
        assert!(!out.join("updates.json").exists());
        fs::create_dir_all(out.join("JSON")).unwrap();
        prepare(&out.join("JSON/x.json"), false).unwrap();
        fs::write(out.join("JSON/x.json"), b"[]").unwrap();
        assert!(rollback().is_empty());
        assert_eq!(fs::read(out.join("CSV/POS/POS_LC.csv")).unwrap(), b"ancien run");
        assert!(out.join("updates.json").exists() && !out.join("JSON").exists() && !out.join(PREVIOUS_DIR).exists());

        // Remise à zéro puis run réussi : ancien contenu supprimé
        begin();
        set_aside_output_directory(&out).unwrap();
        fs::write(out.join("updates.json"), b"{\"a\": 1}").unwrap();
        commit();
        assert!(!out.join(PREVIOUS_DIR).exists() && !out.join("CSV").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
