# -*- coding: utf-8 -*-
"""Cohérence entre l'interface (GUI/app/app.vue), le serveur Python (FragHub.py) et la ligne de commande.

L'interface envoie ses réglages au serveur (POST /run-analysis), qui les valide avec FragHubParams. Ces tests
vérifient, sans lancer l'interface :
- que chaque réglage de l'interface est attendu par le serveur, et inversement ;
- que les réglages par défaut de l'interface sont acceptés par le serveur ;
- que l'interface et la ligne de commande (FragHub.py --cli) ont les mêmes valeurs par défaut : un même jeu de
  données traité sans rien changer donne le même résultat dans les deux.
Les tests de l'interface elle-même (clics, champs, écran de progression) sont dans GUI/tests (Vitest).
"""
import json
import os
import re

import pytest

pytest.importorskip("fraghub_rust", reason="moteur Rust non installé (python build_backend.py --rust-only)")
pytest.importorskip("fastapi", reason="dépendances de FragHub non installées (uv sync)")

import FragHub as fh  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def interface_defaults():
    """Réglages par défaut de l'interface : l'objet donné à useState('parameters', ...) dans app.vue."""
    with open(os.path.join(ROOT, "GUI", "app", "app.vue"), encoding="utf-8") as f:
        text = f.read()
    block = re.search(r"useState\('parameters',\s*\(\)\s*=>\s*\(\{(.*?)\}\)\)", text, re.S)
    assert block, "objet des réglages introuvable dans app.vue"
    values = {}
    for key, value in re.findall(r"^\s*(\w+)\s*:\s*(.+?),?\s*$", block.group(1), re.M):
        values[key] = json.loads(value.replace("'", '"'))
    assert len(values) > 10, "lecture des réglages de app.vue incomplète"
    return values


def cli_defaults():
    """Valeurs par défaut de FragHub.py --cli, converties comme le fait FragHub.py (yes / no -> 1.0 / 0.0)."""
    with open(os.path.join(ROOT, "scripts", "FragHub.py"), encoding="utf-8") as f:
        text = f.read()
    values = {}
    for name, default in re.findall(r'add_argument\("--(\w+)",[^\n]*?default=([^,)\n]+)', text):
        default = default.strip()
        if default in ("'yes'", '"yes"'):
            values[name] = 1.0
        elif default in ("'no'", '"no"'):
            values[name] = 0.0
        else:
            values[name] = float(default)
    assert len(values) > 10, "lecture des arguments de FragHub.py incomplète"
    return values


def server_fields():
    """Noms des réglages attendus par le serveur (alias compris : « json » pour json_enabled)."""
    return {(f.alias or name) for name, f in fh.FragHubParams.model_fields.items()}


def test_reglages_de_l_interface_attendus_par_le_serveur():
    gui = set(interface_defaults()) | {"input_db_names"}     # ajouté par l'onglet INPUT au choix des fichiers
    assert gui == server_fields()


def test_reglages_par_defaut_acceptes_par_le_serveur():
    params = fh.FragHubParams(**dict(interface_defaults(), input_directory=["a.msp"], output_directory="sortie"))
    assert params.json_enabled == 1.0 and params.mzspeclib_json == 1.0


def test_memes_valeurs_par_defaut_que_la_ligne_de_commande():
    gui, cli = interface_defaults(), cli_defaults()
    numeric = {k: v for k, v in gui.items() if isinstance(v, float)}
    assert set(numeric) == set(cli), "réglage présent dans un seul des deux : %s" % (set(numeric) ^ set(cli))
    assert {k: v for k, v in numeric.items() if cli[k] != v} == {}
