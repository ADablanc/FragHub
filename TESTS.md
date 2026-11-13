# Tests de FragHub

Ce guide explique comment lancer les tests automatiques de FragHub, lire leur résultat et en écrire de nouveaux.

## Le principe

Un **test unitaire** est une petite fonction qui appelle une fonction de FragHub avec une entrée choisie, puis
vérifie le résultat. Par exemple :

```python
def test_charge_de_l_adduit():
    assert adduct_charge("[M+2H]2+") == 2
```

Si le résultat n'est pas celui attendu, le test **échoue** et dit ce qu'il a obtenu.

- On écrit un test par comportement important, et en particulier un test pour chaque bug corrigé, avec l'exemple
  réel qui l'avait révélé : le bug ne peut plus revenir sans qu'on le voie.
- Tous les tests se lancent d'un coup, en quelques secondes. On les lance après chaque modification et avant chaque
  commit : si une correction en casse une autre, un test le montre tout de suite. C'est le filet de
  **non-régression**.
- Un test ne touche jamais aux vraies données. Il fabrique ses petites entrées dans un dossier temporaire, effacé
  ensuite, et n'utilise pas Internet : un petit serveur local remplace Zenodo quand il faut.

Il y a trois sortes de tests, du plus petit au plus large :

| Sorte | Ce qu'il vérifie | Durée |
|---|---|---|
| Test unitaire | une fonction, avec une entrée choisie | quelques millisecondes |
| Test du moteur complet (`tests/test_pipeline.py`) | un run entier, de la lecture à l'écriture, sur quelques spectres écrits à la main | moins d'une seconde par run |
| Test de bout en bout (`tests/test_bout_en_bout.py`) | tout FragHub sur de vraies données, comparé à une sortie de référence | moins d'une minute |

FragHub a trois langages, donc trois outils :

| Partie | Outil | Où sont les tests |
|---|---|---|
| Moteur Rust (`scripts/fraghub_rust`) | `cargo test` | dans chaque fichier `.rs`, à la fin, dans le bloc `#[cfg(test)] mod tests` |
| Python (`scripts/`, scripts de build) | `pytest` | dossier `tests/`, fichiers `test_*.py` |
| Interface (`GUI/`) | `vitest` | à venir |

## Ce qu'il faut installer (une seule fois)

Les mêmes outils que pour compiler FragHub depuis le code source (voir `documentation/Documentation.md`) :
- Rust (`cargo`) et RDKit C++ dans `~/rdkit_cpp`, ou dans le dossier donné par la variable `RDKIT_PREFIX` ;
- l'environnement Python du projet : `uv sync` à la racine. Il installe aussi `pytest` et ce que les tests
  utilisent (groupe `dev` du `pyproject.toml`). `uv sync` peut recompiler le module Rust, ce qui prend quelques
  minutes.

## Lancer les tests

Depuis la racine du dépôt :

```powershell
python run_tests.py                 # tout
python run_tests.py --rust          # seulement le moteur Rust
python run_tests.py --python        # seulement les tests Python
python run_tests.py --rust adduit   # seulement les tests dont le nom contient « adduit »
```

Sous Windows, utiliser le Python du projet : `.\.venv\Scripts\python.exe run_tests.py`.

`run_tests.py` règle lui-même ce dont les tests ont besoin, il n'y a rien à taper avant :
- **Windows** : les DLL de RDKit C++ et de Python (`python3xx.dll`) sont mises en tête du PATH. RDKit C++ est placé
  avant tout autre RDKit, par exemple celui d'un environnement conda, qui a des DLL du même nom.
- **macOS / Linux** : l'édition de liens avec `libpython`, que PyO3 ne fait pas pour un module Python.

La première fois, `cargo test` compile tout le moteur en mode test : plusieurs minutes. Ensuite, seuls les
fichiers modifiés sont recompilés.

On peut aussi lancer chaque outil à la main :
- `cd scripts/fraghub_rust` puis `cargo test --lib`, une fois le PATH réglé comme ci-dessus ;
- `python -m pytest` depuis la racine, avec le Python du `.venv` ;
- `python -m pytest tests/test_internal_database.py -v` pour un seul fichier, en listant chaque test.

## Lire le résultat

À la fin, `run_tests.py` affiche une ligne par partie :

```
✔ rust : tous les tests passent
✘ python : au moins un test échoue (voir plus haut)
```

Plus haut, chaque outil donne le détail de ce qui a échoué. Pour pytest :

```
FAILED tests/test_internal_database.py::test_reprise_apres_coupure
    assert handler.requests == ["bytes=1500-"]
E   AssertionError: assert [None] == ['bytes=1500-']
```

- La première ligne donne le fichier et le nom du test.
- La ligne `assert` est la vérification qui a échoué.
- La ligne `E` donne ce qui a été obtenu, à gauche, et ce qui était attendu, à droite.

Pour cargo :

```
---- normalizer::normalize_name::tests::energie_lue_dans_le_commentaire stdout ----
assertion `left == right` failed
  left: "30"
 right: "30 eV"
```

Ici, `left` est la valeur obtenue et `right` la valeur attendue.

Un test qui échoue veut dire l'une de ces deux choses :
- **soit le code a changé de comportement sans le vouloir** : il faut corriger le code ;
- **soit le changement est voulu** : il faut alors mettre le test à jour, en le disant dans le commit.

Les tests marqués `ignored` dans la sortie de cargo ont besoin de la vraie base interne (1 Go) et ne se lancent pas
par défaut. Pour les lancer : `cargo test --lib -- --ignored`, avec le PATH réglé.

## Le test de bout en bout

`tests/test_bout_en_bout.py` fait tourner **tout FragHub** sur un petit jeu d'entrée réel (`tests/data/bout_en_bout/entree` :
quelques spectres de chaque source et de chaque format, plus un tirage aléatoire dans les fichiers d'origine). Il
compare ensuite **chaque fichier de sortie, octet par octet**, à une sortie de référence enregistrée
(`tests/data/bout_en_bout/reference`). C'est lui qui garantit qu'une modification ne change pas un résultat sans qu'on
le voie.

- Il a besoin de la vraie base interne, et prend moins d'une minute (surtout le chargement de la base). Sans la
  base, il est sauté. Pour lancer les autres tests sans lui : `python run_tests.py --fast`.
- S'il échoue, il donne le fichier qui diffère et les premières lignes différentes (`-` : référence, `+` : obtenu).
- Si le changement est **voulu** (correction d'un bug, nouvelle version de la base), on régénère la référence, on
  regarde les différences avec git, puis on la commite avec la modification :

```powershell
python -m pytest tests/test_bout_en_bout.py --update-reference
git diff --stat tests/data/bout_en_bout/reference
```

## Les tests du moteur complet

`tests/test_pipeline.py` lance le moteur (`fraghub_rust.main_orchestrator`, comme le fait `FragHub.py`) sur de très
petits fichiers que le test écrit lui-même. La base interne est une **mini-base de deux molécules** (caféine,
quercétine) fabriquée par le test : pas besoin de la vraie base, et un run prend moins d'une seconde.

Chaque test vérifie un résultat précis, par exemple :
- le même spectre écrit en MSP, MGF, JSON, JSONL et CSV donne les mêmes pics et les mêmes champs ;
- chaque motif de suppression envoie le spectre dans le bon fichier de `DELETED_SPECTRUMS` ;
- chacune des douze bibliothèques (POS / NEG × LC / GC / DI × expérimental / in silico) reçoit son spectre ;
- un deuxième run sur le même dossier n'ajoute que les nouveaux spectres ; un arrêt remet le dossier comme avant ;
- un filtre désactivé ne supprime plus rien.

Pour ajouter un cas, on écrit le spectre avec `msp(...)`, `mgf(...)`, `json_object(...)` ou `csv_file(...)`, on
lance `run_ok(...)`, puis on regarde `result.kept` (spectres écrits) et `result.deleted` (spectres supprimés) :

```python
def test_adduit_lu_dans_le_nom(engine, tmp_path):
    source = write(tmp_path, "a.msp", msp("ESSAI_1", Name="Caffeine [M+Na]+", Precursor_type=None, PrecursorMZ="217.0696"))
    result = run_ok(engine, tmp_path / "out", [source])
    assert result.kept["ESSAI_1"]["PRECURSORTYPE"] == "[M+Na]+"
```

- `msp("ESSAI_1", ...)` part d'un spectre correct de caféine, d'identifiant `ESSAI_1` ; chaque argument remplace
  une ligne, `None` la retire.
- L'identifiant ne doit pas être une valeur que FragHub prend pour un champ vide (`X`, `NA`, `0`…).
- Deux spectres d'essai n'ont jamais les mêmes pics (sinon l'un serait supprimé comme doublon de l'autre), sauf si
  on donne `peaks=` soi-même.

### Le moteur compilé doit être à jour

Les tests Python qui lancent le moteur (`test_pipeline.py`, `test_bout_en_bout.py`) utilisent le moteur **compilé**
installé dans `.venv`, pas les sources Rust. Après une modification du Rust, il faut donc recompiler :

```powershell
python build_backend.py --rust-only
```

Sinon ces tests vérifieraient l'ancien code. Pour l'éviter, ils **s'arrêtent** si un fichier de
`scripts/fraghub_rust/src` est plus récent que le moteur installé, en donnant le nom du fichier. La première ligne
de pytest donne la date de compilation du moteur. Si seuls des tests Rust ont changé (le code de FragHub est le
même), on peut passer outre : `python -m pytest --allow-stale-engine`.

Les tests Rust (`cargo test`) compilent toujours les sources : ils ne sont pas concernés.

## Écrire un test

### En Python

Créer, ou compléter, un fichier `tests/test_<module>.py`. Chaque fonction dont le nom commence par `test_` est un
test :

```python
import internal_database as idb

def test_dossier_vide_compte_comme_absent(tmp_path):
    ref = {"files": [{"name": "part_0000.parquet", "size": 10}]}
    (tmp_path / "datas" / "internal_databases.parquet").mkdir(parents=True)
    assert idb.local_state(str(tmp_path), ref) == "absente"
```

- `tmp_path` est un dossier temporaire neuf, fourni par pytest à tout test qui le demande en paramètre.
- `pytest.raises(Erreur, match="texte")` vérifie qu'une erreur est bien levée, avec le bon message.
- `monkeypatch` remplace une fonction le temps d'un test (par exemple `time.sleep`, pour ne pas attendre).
- Les réglages communs et le petit serveur HTTP local sont dans `tests/conftest.py`.

### En Rust

À la fin du fichier `.rs` du module testé (exemple réel, `normalizer/normalize_ms_level.rs`) :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Petite aide : niveau MS lu à partir d'une seule valeur brute
    fn level(raw: &str) -> String {
        let mut m = HashMap::new();
        m.insert("MSLEVEL".to_string(), raw.to_string());
        normalize_ms_level(m)["MSLEVEL"].clone()
    }

    #[test]
    fn niveau_ms_reduit_au_chiffre() {
        assert_eq!(level("MS2"), "2");
        assert_eq!(level("MS2/MS3"), "2-3");
    }
}
```

- `#[cfg(test)]` : ce bloc n'est compilé que pour les tests, jamais dans FragHub lui-même.
- `use super::*;` donne accès à tout le module, y compris à ses fonctions privées.
- `assert_eq!(obtenu, attendu)` et `assert!(condition, "message")` sont les vérifications.
- Fonction qui prend le jeton `py: Python` (lecteurs, tri, étapes du pipeline) : l'appeler dans
  `crate::loaders::test_support::with_py(|py| ...)`, qui démarre l'interpréteur Python des tests. Les callbacks de
  progression peuvent rester à `None`.
- Fichier d'entrée de test : `crate::loaders::test_support::temp_file("nom_du_test", "lib.mgf", contenu)`.
- Test qui demande l'arrêt (`crate::stop::request()`) : prendre d'abord `crate::stop::TEST_LOCK`, comme le test de
  `stop.rs`. Pendant les tests, un arrêt demandé n'est vu que par le test qui l'a demandé : les autres tests, qui
  tournent en même temps, ne sont pas interrompus.

### Les règles

- **Un test, un comportement**, avec un nom qui le dit : `inchi_valide_pas_remplacee_par_une_valeur_qui_n_en_est_pas_une`.
- **Des exemples réels** quand il y en a : un extrait du fichier MoNA ou GNPS qui a posé problème, avec son
  identifiant (`MoNA022204`) dans un commentaire.
- **Indépendant** : un test ne dépend ni d'un autre test, ni de l'ordre, ni de la machine. Il n'utilise ni
  Internet, ni la vraie base, ni les données de `DATAS/`.
- **Rapide** : moins d'une seconde.
- **Un bug corrigé = un test** qui échouait avant la correction et passe après.

## Quand les lancer

- après chaque modification du code, au moins la partie concernée (`--rust` ou `--python`) ;
- avant chaque commit et avant chaque fusion de branche : `python run_tests.py`.

## Dépannage

| Message | Cause | Solution |
|---|---|---|
| `STATUS_DLL_NOT_FOUND` (`0xc0000135`) | `cargo test` lancé à la main sans le PATH | passer par `run_tests.py` |
| `RDKit C++ introuvable` | RDKit C++ n'est pas dans `~/rdkit_cpp` | l'installer, ou donner son dossier dans `RDKIT_PREFIX` |
| `No module named pytest` (ou `httpx`, `pyarrow`) | outils de test pas installés | `uv sync` à la racine |
| `Environnement .venv introuvable` | projet jamais installé | `uv sync` à la racine |
| `Le moteur Rust installé (...) est plus ancien que ...` | source Rust modifiée depuis la dernière compilation | `python build_backend.py --rust-only` |
