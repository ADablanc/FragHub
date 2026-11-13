# -*- coding: utf-8 -*-
"""Test de bout en bout : un petit jeu d'entrée réel passe dans tout FragHub, et chaque fichier de sortie est comparé,
octet par octet, à une sortie de référence enregistrée.

Le jeu d'entrée (tests/data/bout_en_bout/entree) : quelques spectres de chaque source et de chaque format lus par
FragHub (MoNA MSP et JSON, GNPS JSON, MGF et MSP, MassBank MSP aux formats NIST et RIKEN, MSMS-Public, MetaboAnalyst
CSV, MSnLib MGF, Spectraverse, JSON et JSONL, spectres in silico), plus les quatre Acyl-CoA de MoNA du bug
« inchi=1 ». Voir le README.md du dossier.

La référence (tests/data/bout_en_bout/reference) est la sortie d'un run validé. Toute modification qui change une
sortie (un nom, une classe, un pic, un spectre supprimé…) fait échouer ce test, avec les premières lignes qui
diffèrent. Si le changement est voulu, on régénère la référence et on la commite avec la modification :

    python -m pytest tests/test_bout_en_bout.py --update-reference

Le run utilise le moteur Rust COMPILÉ installé dans .venv : si une source Rust est plus récente, le test s'arrête
en le disant (fixture moteur_a_jour de conftest.py ; recompiler avec python build_backend.py --rust-only).
Le test a besoin de la vraie base interne (datas/internal_databases.parquet, version de référence) : il est sauté
si elle manque. Les noms et les classes viennent de la base : une nouvelle version de la base demande une nouvelle
référence (le test le dit). Durée : une à deux minutes (chargement de la base).
Lancer les autres tests sans lui : python run_tests.py --python --fast
"""
import difflib
import os
import shutil
import subprocess
import sys

import pytest

import internal_database

pytestmark = pytest.mark.bout_en_bout

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "tests", "data", "bout_en_bout")
INPUT = os.path.join(DATA, "entree")
REFERENCE = os.path.join(DATA, "reference")
BASE_VERSION_FILE = "_version_de_la_base.txt"
# Fichiers de sortie qui changent d'un run à l'autre sans que le résultat change : non comparés
IGNORED = ("PERF_LOGS",)


def ignored(rel):
    name = os.path.basename(rel)
    return (rel.split(os.sep)[0] in IGNORED or name.startswith("report_") and name.endswith(".html")
            or name == "updates.json")


def output_files(folder):
    found = {}
    for root, _, files in os.walk(folder):
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), folder)
            if not ignored(rel) and rel != BASE_VERSION_FILE:
                found[rel.replace(os.sep, "/")] = os.path.join(root, f)
    return found


def first_differences(expected, actual, n=25):
    a = expected.decode("utf-8", "replace").splitlines()
    b = actual.decode("utf-8", "replace").splitlines()
    diff = list(difflib.unified_diff(a, b, "référence", "obtenu", lineterm="", n=0))
    return "\n".join(line[:300] for line in diff[:n]) + ("\n…" if len(diff) > n else "")


@pytest.fixture(scope="module")
def base_version():
    try:
        ref = internal_database.read_reference(ROOT)
    except internal_database.InternalDatabaseError as e:
        pytest.skip("référence de la base illisible : %s" % e)
    if internal_database.local_state(ROOT, ref) != "reference":
        pytest.skip("base interne absente ou différente de la référence (python scripts/internal_database.py)")
    return ref["version"]


@pytest.fixture(scope="module")
def run_output(tmp_path_factory, base_version, moteur_a_jour):
    if not os.path.isdir(INPUT) or not os.listdir(INPUT):
        pytest.fail("jeu d'entrée absent : %s" % INPUT)
    out = tmp_path_factory.mktemp("sortie_fraghub")
    cmd = [sys.executable, os.path.join(ROOT, "scripts", "FragHub.py"), "--cli", "--quiet",
           "--input_directory", INPUT, "--output_directory", str(out), "--calculate_de_novo", "yes"]
    run = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace",
                         timeout=1800)
    if run.returncode != 0:
        pytest.fail("FragHub a échoué (code %d) :\n%s\n%s" % (run.returncode, run.stdout[-3000:], run.stderr[-3000:]))
    return str(out)


def test_sorties_identiques_a_la_reference(run_output, base_version, request):
    actual = output_files(run_output)
    assert actual, "FragHub n'a écrit aucun fichier de sortie"

    if request.config.getoption("--update-reference"):
        shutil.rmtree(REFERENCE, ignore_errors=True)
        for rel, path in actual.items():
            dst = os.path.join(REFERENCE, *rel.split("/"))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copyfile(path, dst)
        with open(os.path.join(REFERENCE, BASE_VERSION_FILE), "w", encoding="utf-8", newline="\n") as f:
            f.write(base_version + "\n")
        pytest.skip("référence régénérée (%d fichiers) : vérifier le diff avec git avant de commiter" % len(actual))

    if not os.path.isdir(REFERENCE):
        pytest.fail("pas encore de référence : lancer une fois avec --update-reference, vérifier, puis commiter")
    with open(os.path.join(REFERENCE, BASE_VERSION_FILE), encoding="utf-8") as f:
        ref_version = f.read().strip()
    if ref_version != base_version:
        pytest.fail("la référence a été faite avec la base %s, la base installée est la %s : regénérer la référence "
                    "(--update-reference) après avoir vérifié les différences" % (ref_version, base_version))

    expected = output_files(REFERENCE)
    problems = []
    for rel in sorted(set(expected) - set(actual)):
        problems.append("fichier absent de la sortie : %s" % rel)
    for rel in sorted(set(actual) - set(expected)):
        problems.append("fichier en plus dans la sortie : %s" % rel)
    for rel in sorted(set(expected) & set(actual)):
        with open(expected[rel], "rb") as f:
            exp = f.read()
        with open(actual[rel], "rb") as f:
            act = f.read()
        if exp != act:
            problems.append("%s différent :\n%s" % (rel, first_differences(exp, act)))
    assert not problems, "\n\n".join(problems)


def test_bug_inchi_1_de_mona_corrige(run_output):
    """Les Acyl-CoA de MoNA gardent leur InChI d'origine et leur clé avec stéréochimie (raw_metadata.rs)."""
    import csv
    csv.field_size_limit(2 ** 31 - 1)
    rows = []
    for path in output_files(run_output).values():
        if path.endswith(".csv") and os.sep + "CSV" + os.sep in path:
            with open(path, encoding="utf-8", newline="") as h:
                rows += [r for r in csv.DictReader(h, delimiter=";") if r.get("SPECTRUMID", "").startswith("MoNA02")]
    assert rows, "spectres Acyl-CoA de MoNA absents de la sortie"
    by_id = {r["SPECTRUMID"]: r for r in rows}
    assert by_id["MoNA022204"]["INCHIKEY"] == "KQMZYOXOBSXMII-AJIKYXGBSA-N"
    assert all(r["INCHI"].startswith("InChI=1S/") and "/t" in r["INCHI"] for r in rows)
