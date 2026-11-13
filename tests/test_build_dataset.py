# -*- coding: utf-8 -*-
"""Tests des scripts de la base interne (scripts/build_dataset) : make_archive, dedupe_db, add_new_molecules et les
fonctions de npclassifier_db qui n'ont pas besoin de RDKit.

Chaque test fabrique une petite base de quelques lignes (mêmes colonnes et mêmes types que la vraie) dans son
dossier temporaire ; la vraie base n'est jamais lue.
"""
import csv
import hashlib
import json
import os
import sys
import zipfile

import pytest

pa = pytest.importorskip("pyarrow")
import pyarrow.parquet as pq  # noqa: E402

import add_new_molecules  # noqa: E402
import dedupe_db  # noqa: E402
import make_archive  # noqa: E402
import npclassifier_db as npc  # noqa: E402

TEXT = ["INCHIKEY", "INCHI", "SMILES", "FORMULA", "NAME"]
CLASSES = ["CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS",
           "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS"]
COLUMNS = TEXT + ["EXACTMASS", "AVERAGEMASS"] + CLASSES
CF = ("Organoheterocyclic compounds", "Imidazopyrimidines", "Purines and purine derivatives")


def row(key, name="", formula="", inchi="", smiles="", cf=(None, None, None), npc_=(None, None, None), mass=None):
    return dict(zip(COLUMNS, [key, inchi, smiles, formula, name, mass, mass, *cf, *npc_]))


def write_part(path, rows):
    """Un fichier de base : colonnes de texte, masses en nombres, classes en dictionnaires (comme la vraie base)."""
    arrays, fields = [], []
    for c in COLUMNS:
        values = [r[c] for r in rows]
        if c in CLASSES:
            arr = pa.array(values, type=pa.string()).dictionary_encode()
            arr = arr.cast(pa.dictionary(pa.int8(), pa.string()))
        elif c in ("EXACTMASS", "AVERAGEMASS"):
            arr = pa.array(values, type=pa.float64())
        else:
            arr = pa.array(values, type=pa.string())
        arrays.append(arr)
        fields.append(pa.field(c, arr.type))
    pq.write_table(pa.Table.from_arrays(arrays, schema=pa.schema(fields)), str(path), compression="zstd")


def read(path):
    return pq.read_table(str(path)).to_pylist()


def run_main(module, monkeypatch, *args):
    monkeypatch.setattr(sys, "argv", [module.__name__ + ".py", *map(str, args)])
    return module.main()


# =====================================================================================================
# make_archive.py
# =====================================================================================================

@pytest.fixture
def base(tmp_path):
    d = tmp_path / "internal_databases.parquet"
    d.mkdir()
    (d / "part_0000.parquet").write_bytes(b"premier fichier")
    (d / "part_0001.parquet").write_bytes(b"second" * 100)
    (d / "notes.txt").write_text("hors archive")
    return d


def test_archive_et_reference(base, tmp_path, monkeypatch, capsys):
    out = tmp_path / "out"
    run_main(make_archive, monkeypatch, "--base", base, "--version", "2099-01-01", "--record", "123", "--output", out)
    z = out / "internal_database_2099-01-01.zip"
    ref = json.loads((out / "internal_database_2099-01-01.json").read_text(encoding="utf-8"))
    assert ref["url"] == "https://zenodo.org/records/123/files/internal_database_2099-01-01.zip?download=1"
    assert ref["doi"] == "10.5281/zenodo.123"
    assert ref["size"] == os.path.getsize(z) and ref["sha256"] == hashlib.sha256(z.read_bytes()).hexdigest()
    assert [f["name"] for f in ref["files"]] == ["part_0000.parquet", "part_0001.parquet"]
    assert ref["files"][0] == {"name": "part_0000.parquet", "size": 15,
                               "sha256": hashlib.sha256(b"premier fichier").hexdigest()}
    with zipfile.ZipFile(z) as archive:
        assert archive.namelist() == ["part_0000.parquet", "part_0001.parquet"]
        assert archive.read("part_0001.parquet") == b"second" * 100
        assert all(i.compress_type == zipfile.ZIP_STORED for i in archive.infolist())
    assert "notes.txt" in capsys.readouterr().out   # signalé comme laissé hors de l'archive


def test_reference_garde_les_versions_publiees_avant(base, tmp_path, monkeypatch):
    # La référence actuelle (2099-01-01) et sa liste deviennent les versions précédentes de la nouvelle
    root = tmp_path / "depot"
    (root / "datas").mkdir(parents=True)
    current = {"version": "2099-01-01", "doi": "10.5281/zenodo.2", "files": [{"name": "part_0000.parquet", "size": 5, "sha256": "x"}],
               "previous_versions": [{"version": "2098-01-01", "doi": "10.5281/zenodo.1", "files": [{"name": "part_0000.parquet", "size": 4}]}]}
    (root / "datas" / "internal_database.json").write_text(json.dumps(current), encoding="utf-8")
    monkeypatch.setattr(make_archive, "ROOT", str(root))
    run_main(make_archive, monkeypatch, "--base", base, "--version", "2099-02-01", "--output", tmp_path / "o")
    ref = json.loads((tmp_path / "o" / "internal_database_2099-02-01.json").read_text(encoding="utf-8"))
    assert ref["previous_versions"] == [
        {"version": "2099-01-01", "doi": "10.5281/zenodo.2", "files": [{"name": "part_0000.parquet", "size": 5}]},
        {"version": "2098-01-01", "doi": "10.5281/zenodo.1", "files": [{"name": "part_0000.parquet", "size": 4}]}]
    # Même version refaite (archive recalculée) : elle ne se liste pas elle-même
    run_main(make_archive, monkeypatch, "--base", base, "--version", "2099-01-01", "--output", tmp_path / "p")
    ref = json.loads((tmp_path / "p" / "internal_database_2099-01-01.json").read_text(encoding="utf-8"))
    assert [v["version"] for v in ref["previous_versions"]] == ["2098-01-01"]


def test_archive_reproductible(base, tmp_path, monkeypatch):
    sums = []
    for out in ("a", "b"):
        run_main(make_archive, monkeypatch, "--base", base, "--version", "v1", "--output", tmp_path / out)
        sums.append(hashlib.sha256((tmp_path / out / "internal_database_v1.zip").read_bytes()).hexdigest())
    assert sums[0] == sums[1]


def test_archive_sans_numero_de_depot(base, tmp_path, monkeypatch):
    run_main(make_archive, monkeypatch, "--base", base, "--version", "v1", "--output", tmp_path / "o")
    ref = json.loads((tmp_path / "o" / "internal_database_v1.json").read_text(encoding="utf-8"))
    assert ref["url"] == "" and ref["doi"] == ""


@pytest.mark.parametrize("args, message", [
    (["--version", "../x"], "Version refusée"),
    (["--version", "v1", "--record", "abc"], "--record"),
])
def test_archive_arguments_refuses(base, tmp_path, monkeypatch, args, message):
    with pytest.raises(SystemExit, match=message):
        run_main(make_archive, monkeypatch, "--base", base, "--output", tmp_path / "o", *args)


def test_archive_base_vide(tmp_path, monkeypatch):
    (tmp_path / "vide").mkdir()
    with pytest.raises(SystemExit, match="Aucun fichier"):
        run_main(make_archive, monkeypatch, "--base", tmp_path / "vide", "--version", "v1", "--output", tmp_path / "o")


# =====================================================================================================
# dedupe_db.py
# =====================================================================================================

def test_couche_de_charge_de_l_inchi():
    assert dedupe_db.is_charged_inchi("InChI=1S/C2H4O2/c1-2(3)4/h1H3,(H,3,4)/p-1") is True
    assert dedupe_db.is_charged_inchi("InChI=1S/C5H11N/c1-6/q+1") is True
    assert dedupe_db.is_charged_inchi("InChI=1S/C2H4O2/c1-2(3)4/h1H3,(H,3,4)") is False
    assert dedupe_db.is_charged_inchi("") is None and dedupe_db.is_charged_inchi(None) is None


def test_alignement_avec_l_ancienne_base():
    assert dedupe_db.align(["a", "c"], ["a", "b", "c"]) == [0, 2]      # ligne retirée par la re-normalisation
    assert dedupe_db.align(["a", "a"], ["a", "x", "a"]) == [0, 2]
    assert dedupe_db.align(["z"], ["a", "b"]) is None


def test_colonne_dictionnaire_elargie_si_trop_de_valeurs():
    field = pa.field("C", pa.dictionary(pa.int8(), pa.string()))
    values = ["v%d" % i for i in range(300)] + [None]
    col = dedupe_db.dictionary_column(values, field)
    assert col.type.index_type == pa.uint32() and col.to_pylist() == values
    small = dedupe_db.dictionary_column(["a", None, "a"], field)
    assert small.type.index_type == pa.int8() and small.to_pylist() == ["a", None, "a"]


@pytest.fixture
def base_avec_doublons(tmp_path):
    d = tmp_path / "base"
    d.mkdir()
    write_part(d / "part_0000.parquet", [
        row("KEYCLASSES-A", "CID 123", "C9H8O4"),                       # sans classes : perd face à la ligne classée
        row("KEYNOMIDEN-A", "Caffeine", "C8H10N4O2"),                   # nom réel : gardé face à SCHEMBL
        row("KEYFORMULE-A", "Mol A", "C2H6O"),                          # formules différentes : groupe laissé
        row("KEYCLASSEU-A", "Adenine", "C5H5N5"),                       # complète, reprend les classes de la ligne « classes seules »
        row("KEYPREMIER-A", "Premier", "CH4"),                          # égalité : la première lue gagne
        row("KEYSEULE000-A", "Unique", "H2O"),
    ])
    write_part(d / "part_0001.parquet", [
        row("KEYCLASSES-A", "Aspirin", "C9H8O4", cf=CF),
        row("KEYNOMIDEN-A", "SCHEMBL42", "C8H10N4O2"),
        row("KEYFORMULE-A", "Mol B", "C2H6S"),
        row("KEYCLASSEU-A", cf=CF),                                     # ligne « classes seules »
        row("KEYPREMIER-A", "Second", "CH4"),
    ])
    write_part(d / "part_0002.parquet", [row("KEYAUTRE0000-A", "Autre", "N2")])
    return d


def test_dedoublonnage(base_avec_doublons, tmp_path, monkeypatch):
    before = {f: (base_avec_doublons / f).read_bytes() for f in os.listdir(base_avec_doublons)}
    out = tmp_path / "out"
    run_main(dedupe_db, monkeypatch, "--input", base_avec_doublons, "--output", out)
    p0, p1 = read(out / "part_0000.parquet"), read(out / "part_0001.parquet")
    kept = {(r["INCHIKEY"], r["NAME"]) for r in p0 + p1}
    assert ("KEYCLASSES-A", "Aspirin") in kept and ("KEYCLASSES-A", "CID 123") not in kept
    assert ("KEYNOMIDEN-A", "Caffeine") in kept and ("KEYNOMIDEN-A", "SCHEMBL42") not in kept
    assert ("KEYFORMULE-A", "Mol A") in kept and ("KEYFORMULE-A", "Mol B") in kept       # laissé tel quel
    assert ("KEYPREMIER-A", "Premier") in kept and ("KEYPREMIER-A", "Second") not in kept
    adenine = [r for r in p0 if r["INCHIKEY"] == "KEYCLASSEU-A"]
    assert len(adenine) == 1 and adenine[0]["NAME"] == "Adenine"
    assert (adenine[0]["CLASSYFIRE_SUPERCLASS"], adenine[0]["CLASSYFIRE_CLASS"], adenine[0]["CLASSYFIRE_SUBCLASS"]) == CF
    assert not [r for r in p1 if r["INCHIKEY"] == "KEYCLASSEU-A"]                         # ligne « classes seules » retirée
    # une clé par ligne, sauf le groupe laissé
    keys = [r["INCHIKEY"] for r in p0 + p1]
    assert sorted(k for k in set(keys) if keys.count(k) > 1) == ["KEYFORMULE-A"]
    # fichier sans doublon : recopié octet pour octet ; base d'origine intacte
    assert (out / "part_0002.parquet").read_bytes() == before["part_0002.parquet"]
    assert {f: (base_avec_doublons / f).read_bytes() for f in os.listdir(base_avec_doublons)} == before
    with open(out / "_dedupe_lignes_retirees.tsv", encoding="utf-8") as h:
        removed = list(csv.DictReader(h, delimiter="\t"))
    criteria = {r["INCHIKEY"]: r["critere"] for r in removed}
    assert criteria["KEYCLASSES-A"] == "a des classes ClassyFire"
    assert criteria["KEYNOMIDEN-A"] == "nom non identifiant"
    assert criteria["KEYPREMIER-A"] == "première dans l'ordre de lecture"
    assert criteria["KEYCLASSEU-A"] == "ligne complète (l'autre n'a que des classes)"


def test_dedoublonnage_ligne_deja_neutre_avant_renormalisation(tmp_path, monkeypatch):
    base, old, out = tmp_path / "base", tmp_path / "ancienne", tmp_path / "out"
    base.mkdir()
    old.mkdir()
    # Après re-normalisation, les deux lignes ont la même clé ; avant, la première était chargée (/p-1)
    write_part(base / "part_0000.parquet", [row("KEYNEUTRE000-A", "Acetate", "C2H4O2", cf=CF),
                                            row("KEYNEUTRE000-A", "Acetic acid", "C2H4O2")])
    write_part(old / "part_0000.parquet", [row("X", "Acetate", inchi="InChI=1S/C2H4O2/c1-2(3)4/h1H3,(H,3,4)/p-1"),
                                           row("X", "Acetic acid", inchi="InChI=1S/C2H4O2/c1-2(3)4/h1H3,(H,3,4)")])
    run_main(dedupe_db, monkeypatch, "--input", base, "--output", out, "--old-dir", old)
    rows = read(out / "part_0000.parquet")
    assert [r["NAME"] for r in rows] == ["Acetic acid"]
    assert rows[0]["CLASSYFIRE_CLASS"] == CF[1]   # classes reprises de la ligne retirée


def test_dedoublonnage_constat_seul(base_avec_doublons, tmp_path, monkeypatch):
    out = tmp_path / "out"
    run_main(dedupe_db, monkeypatch, "--input", base_avec_doublons, "--output", out, "--dry-run")
    assert not [f for f in os.listdir(out) if f.endswith(".parquet")]
    assert "Constat seul" in (out / "_dedupe_journal.txt").read_text(encoding="utf-8")


def test_dedoublonnage_refuse_d_ecrire_sur_la_base(base_avec_doublons, monkeypatch):
    with pytest.raises(SystemExit, match="nouveau dossier"):
        run_main(dedupe_db, monkeypatch, "--input", base_avec_doublons, "--output", base_avec_doublons)


# =====================================================================================================
# add_new_molecules.py (constat seul : sans COCONUT ni modèle NP-Classifier)
# =====================================================================================================

def test_valeur_la_plus_frequente_premiere_a_egalite():
    from collections import Counter
    assert add_new_molecules.most_common_first(Counter(["b", "a", "a"])) == "a"
    assert add_new_molecules.most_common_first(Counter(["b", "a"])) == "b"
    assert add_new_molecules.most_common_first(Counter()) is None


def test_classyfire_repris_d_une_voisine_de_stereochimie_differente(tmp_path):
    d = tmp_path / "base"
    d.mkdir()
    write_part(d / "part_0000.parquet", [
        row("AAAAAAAAAAAAAA-BBBBBBBBSA-N", cf=CF),                        # voisine : même 1er bloc, même protonation
        row("CCCCCCCCCCCCCC-DDDDDDDDSA-N", cf=CF),                        # deux voisines en désaccord
        row("CCCCCCCCCCCCCC-EEEEEEEESA-N", cf=("Lipids", "Fatty Acyls", "x")),
        row("FFFFFFFFFFFFFF-GGGGGGGGSA-M", cf=CF),                        # protonation différente (M) : pas une voisine
        row("HHHHHHHHHHHHHH-IIIIIIIISA-N", cf=("NOT FOUND", None, None)), # sans classes : ignorée
    ])
    keys = ["AAAAAAAAAAAAAA-UHFFFAOYSA-N", "CCCCCCCCCCCCCC-UHFFFAOYSA-N", "FFFFFFFFFFFFFF-UHFFFAOYSA-N",
            "HHHHHHHHHHHHHH-UHFFFAOYSA-N"]
    inherited, conflicts = add_new_molecules.classyfire_from_neighbours(str(d), ["part_0000.parquet"], keys)
    assert inherited == {"AAAAAAAAAAAAAA-UHFFFAOYSA-N": (CF, "AAAAAAAAAAAAAA-BBBBBBBBSA-N")}
    assert conflicts == 1


def write_run_csv(path, rows):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    head = ["FILENAME", "NAME", "INCHIKEY", "INCHI", "SMILES", "FORMULA", "EXACTMASS", "AVERAGEMASS"]
    with open(path, "w", newline="", encoding="utf-8") as h:
        w = csv.writer(h, delimiter=";")
        w.writerow(head)
        for r in rows:
            w.writerow([r.get(c, "") for c in head])


def test_molecules_nouvelles_constat_seul(tmp_path, monkeypatch):
    base, run, out = tmp_path / "base", tmp_path / "run", tmp_path / "out"
    base.mkdir()
    write_part(base / "part_0000.parquet", [row("DEJAPRESENTE-A", "Connue", "CH4"),
                                            row("NOUVELLEMOLECU-BBBBBBBBSA-N", cf=CF)])
    new = {"INCHIKEY": "NOUVELLEMOLECU-UHFFFAOYSA-N", "SMILES": "CCO", "FORMULA": "C2H6O", "INCHI": "InChI=1S/C2H6O"}
    write_run_csv(run / "CSV" / "POS" / "POS_LC.csv", [
        dict(new, FILENAME="a.msp", NAME="Ethanol"),
        dict(new, FILENAME="b.mgf", NAME="Ethanol"),
        dict(new, FILENAME="a.msp", NAME="Alcool"),
        {"FILENAME": "a.msp", "NAME": "Connue", "INCHIKEY": "DEJAPRESENTE-A"},
        {"FILENAME": "a.msp", "NAME": "sans clé", "INCHIKEY": "NOT FOUND"},
    ])
    write_run_csv(run / "CSV" / "NEG" / "NEG_LC.csv", [dict(new, FILENAME="c.json", NAME="Alcool")])
    run_main(add_new_molecules, monkeypatch, "--run", run, "--input", base, "--output", out, "--dry-run")
    with open(out / "_nouvelles_molecules.tsv", encoding="utf-8") as h:
        rows = list(csv.DictReader(h, delimiter="\t"))
    assert len(rows) == 1
    r = rows[0]
    assert r["INCHIKEY"] == "NOUVELLEMOLECU-UHFFFAOYSA-N" and r["spectres"] == "4"
    # « Alcool » 2 fois, « Ethanol » 2 fois : à égalité, le premier rencontré (CSV/NEG est lu avant CSV/POS)
    assert r["NAME"] == "Alcool" and r["autres_noms"] == "1"
    assert r["bibliotheques"] == "NEG_LC|POS_LC" and r["fichiers_entree"] == "a.msp|b.mgf|c.json"
    assert r["classyfire_repris_de"] == "NOUVELLEMOLECU-BBBBBBBBSA-N" and r["CLASSYFIRE_CLASS"] == CF[1]
    assert not [f for f in os.listdir(out) if f.endswith(".parquet")]
    journal = (out / "_journal.txt").read_text(encoding="utf-8")
    assert "spectres dont la molécule est dans la base" in journal


def test_molecules_nouvelles_sans_coconut_refuse(tmp_path, monkeypatch):
    with pytest.raises(SystemExit, match="--coconut"):
        run_main(add_new_molecules, monkeypatch, "--run", tmp_path, "--output", tmp_path / "o")


# =====================================================================================================
# npclassifier_db.py : fonctions sans RDKit
# =====================================================================================================

def test_cellule_triee_ou_not_found():
    assert npc.cell(["Terpenoids", "Alkaloids"]) == "Alkaloids|Terpenoids"
    assert npc.cell([]) == "NOT FOUND"


def test_noms_coconut_decoupes_selon_l_index():
    known = {"Alkaloids", "Terpenoids", "Carotenoids (C40, β-β)"}
    assert npc.split_names("Alkaloids", known) == ["Alkaloids"]
    assert npc.split_names("Alkaloids|Terpenoids", known) == ["Alkaloids", "Terpenoids"]
    assert npc.split_names("Carotenoids (C40, β-β), Alkaloids", known) == ["Carotenoids (C40, β-β)", "Alkaloids"]
    assert npc.split_names("", known) == []
    assert npc.split_names("Inconnu", known) is None


def test_lecture_de_coconut(tmp_path):
    index = {"Pathway": {"Alkaloids": 0, "Terpenoids": 1}, "Superclass": {"Pseudoalkaloids": 0},
             "Class": {"Purine alkaloids": 0}}
    path = tmp_path / "coconut.csv"
    with open(path, "w", newline="", encoding="utf-8") as h:
        w = csv.writer(h)
        w.writerow(["standard_inchi_key", "np_classifier_pathway", "np_classifier_superclass", "np_classifier_class"])
        w.writerow(["KEY1-A", "Terpenoids|Alkaloids", "Pseudoalkaloids", "Purine alkaloids"])
        w.writerow(["KEY2-A", "", "", ""])                       # sans voie : ignorée
        w.writerow(["KEY3-A", "Alkaloids", "Classe inconnue", ""])
    coco, unknown = npc.read_coconut(str(path), index)
    assert coco["KEY1-A"] == ("Alkaloids|Terpenoids", "Pseudoalkaloids", "Purine alkaloids")
    assert "KEY2-A" not in coco
    assert coco["KEY3-A"] == ("Alkaloids", "Classe inconnue", "NOT FOUND") and unknown == 1


def test_fichier_de_reprise_ligne_coupee_ignoree(tmp_path):
    path = tmp_path / "part_0000.parquet.reprise"
    path.write_text("0\tAlkaloids\tA\tB\t.\n1\tTerpenoids\tC\tD\t.\n2\tCoupé", encoding="utf-8")
    assert npc.read_checkpoint(str(path)) == {0: ("Alkaloids", "A", "B"), 1: ("Terpenoids", "C", "D")}
    assert npc.read_checkpoint(str(tmp_path / "absent")) == {}


def test_empreintes_creuses_reconstruites():
    import numpy as np
    sparse = [(np.array([0, 6143], dtype=np.uint16), np.array([2, 5], dtype=np.int32))]
    x = npc.dense_batch(sparse)
    assert x.shape == (1, 6144) and x[0, 0] == 2 and x[0, 6143] == 5 and x.sum() == 7


def test_version_de_rdkit_verifiee(monkeypatch):
    fake = type(sys)("rdkit")
    monkeypatch.setitem(sys.modules, "rdkit", fake)
    fake.__version__ = "2020.03.6"
    assert npc.check_rdkit() == "2020.03.6"
    fake.__version__ = "2022.03.1"   # hachage des empreintes Morgan changé : refusé
    with pytest.raises(SystemExit, match="2022.03"):
        npc.check_rdkit()
