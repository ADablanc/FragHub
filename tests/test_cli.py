# -*- coding: utf-8 -*-
"""Tests des lignes de commande : FragHub.py --cli (arguments refusés avant tout traitement) et run_tests.py.

Le run complet en ligne de commande est fait par le test de bout en bout (test_bout_en_bout.py) : ici, seulement
ce qui s'arrête avant le chargement de la base interne, donc en une ou deux secondes par test.
"""
import os
import subprocess
import sys

import pytest

pytest.importorskip("fraghub_rust", reason="moteur Rust non installé (python build_backend.py --rust-only)")
pytest.importorskip("fastapi", reason="dépendances de FragHub non installées (uv sync)")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def launch(script, *args):
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    return subprocess.run([sys.executable, os.path.join(ROOT, script), *[str(a) for a in args]], cwd=ROOT, env=env,
                          capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)


def fraghub(*args):
    return launch(os.path.join("scripts", "FragHub.py"), "--cli", "--quiet", *args)


def test_aucun_fichier_lisible(tmp_path):
    """Dossier sans fichier .msp / .mgf / .csv / .json / .jsonl : message, code 1, et rien n'est créé."""
    (tmp_path / "entree").mkdir()
    (tmp_path / "entree" / "notes.txt").write_text("pas un spectre", encoding="utf-8")
    run = fraghub("--input_directory", tmp_path / "entree", "--output_directory", tmp_path / "sortie")
    assert run.returncode == 1
    assert "[ERROR] Aucun fichier MS valide" in run.stdout
    assert not (tmp_path / "sortie").exists()


def test_chemin_d_entree_introuvable(tmp_path):
    run = fraghub("--input_directory", tmp_path / "absent", "--output_directory", tmp_path / "sortie")
    assert run.returncode == 1
    assert "[WARNING] Le chemin spécifié est introuvable" in run.stdout
    assert not (tmp_path / "sortie").exists()


@pytest.mark.parametrize("arguments, message", [
    (["--input_directory", "x"], "--output_directory"),                                  # dossier de sortie absent
    (["--output_directory", "x"], "--input_directory"),                                  # entrée absente
    (["--input_directory", "x", "--output_directory", "y", "--csv", "peut-etre"], "--csv"),            # ni yes ni no
    (["--input_directory", "x", "--output_directory", "y", "--de_novo_ppm_tolerance", "dix"], "--de_novo_ppm_tolerance"),
    (["--input_directory", "x", "--output_directory", "y", "--option_inconnue", "1"], "--option_inconnue"),
])
def test_arguments_refuses(arguments, message):
    """Argument manquant, valeur hors liste ou option inconnue : refus (code 2) qui nomme l'argument en cause."""
    run = fraghub(*arguments)
    assert run.returncode == 2
    assert message in run.stderr


def test_run_tests_affiche_les_commandes_sans_les_lancer():
    run = launch("run_tests.py", "--dry-run")
    assert run.returncode == 0, run.stdout + run.stderr
    assert "test --lib" in run.stdout and "-m pytest tests" in run.stdout
    # Une seule partie, sans le test de bout en bout, avec un filtre sur le nom des tests
    run = launch("run_tests.py", "--dry-run", "--python", "--fast", "adduit")
    assert run.returncode == 0, run.stdout + run.stderr
    assert "test --lib" not in run.stdout
    assert "-m pytest tests -k adduit -m not bout_en_bout" in run.stdout


def test_run_tests_aide():
    run = launch("run_tests.py", "--help")
    assert run.returncode == 0
    assert "--fast" in run.stdout and "--dry-run" in run.stdout
