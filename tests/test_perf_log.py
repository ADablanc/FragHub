# -*- coding: utf-8 -*-
"""Tests de scripts/perf_log.py : découpage du run en tâches à partir des messages du moteur, et fichiers écrits."""
import csv
import os

import pytest

import perf_log


@pytest.fixture(autouse=True)
def journal_propre(monkeypatch, tmp_path):
    """Chaque test part sans run en cours, et sans étiquette ni dossier imposés par l'environnement."""
    monkeypatch.delenv("FRAGHUB_PERF_DIR", raising=False)
    monkeypatch.delenv("FRAGHUB_PERF_LABEL", raising=False)
    monkeypatch.setattr(perf_log, "_init", None)
    yield
    if perf_log._state is not None:
        perf_log._state.stop.set()
    perf_log._state = None


def lire_csv(path):
    with open(path, encoding="utf-8-sig", newline="") as f:
        return list(csv.reader(f, delimiter=";"))


def test_rien_n_est_note_sans_run():
    perf_log.on_step("Parsing")
    perf_log.on_total(10)
    perf_log.on_progress(5)
    assert perf_log.finish() is None


def test_trois_messages_completent_une_seule_tache():
    perf_log.start({})
    perf_log.on_step("Cleaning")
    # ordre variable selon les modules : total, libellé, unité
    perf_log.on_prefix("Cleaning spectra:")
    perf_log.on_total(100)
    perf_log.on_item_type("spectra")
    perf_log.on_progress(50)
    perf_log.on_progress(100)
    tasks = [t for t in perf_log._state.tasks if t.prefix != "(entre deux tâches)"]
    assert len(tasks) == 1
    t = tasks[0]
    assert (t.step, t.prefix, t.total, t.item_type, t.last_value, t.progress_events) == \
        ("Cleaning", "Cleaning spectra:", 100, "spectra", 100, 2)


def test_message_apres_une_progression_ouvre_une_nouvelle_tache():
    perf_log.start({})
    perf_log.on_prefix("Lecture")
    perf_log.on_total(10)
    perf_log.on_progress(10)
    perf_log.on_prefix("Écriture")          # déjà une progression : nouvelle tâche
    perf_log.on_total(20)
    perf_log.on_prefix("Writing deletion logs...")   # libellé déjà donné : encore une nouvelle tâche
    prefixes = [t.prefix for t in perf_log._state.tasks if t.prefix != "(entre deux tâches)"]
    assert prefixes == ["Lecture", "Écriture", "Writing deletion logs..."]


def test_nouveau_run_remplace_l_ancien():
    perf_log.start({"output_directory": "a"})
    ancien = perf_log._state
    perf_log.start({"output_directory": "b"})
    assert perf_log._state is not ancien and ancien.stop.is_set()


def test_fichiers_du_journal(tmp_path, monkeypatch):
    monkeypatch.setenv("FRAGHUB_PERF_LABEL", "essai")
    entree = tmp_path / "entree.msp"
    entree.write_bytes(b"x" * 2 * 1024 * 1024)
    sortie = tmp_path / "sortie"
    perf_log.record_init(1.5)
    perf_log.start({"output_directory": str(sortie), "input_directory": [str(entree)], "csv": 1.0})
    perf_log.on_step("Parsing")
    perf_log.on_prefix("Parsing MSP:")
    perf_log.on_total(3)
    perf_log.on_progress(3)
    perf_log.on_deletion("2 spectres supprimés : pics vides")
    perf_log.on_completion("Traitement terminé")
    txt = perf_log.finish()

    dossier = sortie / "PERF_LOGS"
    assert txt is not None and os.path.dirname(txt) == str(dossier)
    noms = sorted(os.listdir(dossier))
    assert noms[0] == "historique.csv" and noms[1].startswith("perf_") and noms[1].endswith("_essai.csv")
    texte = open(txt, encoding="utf-8").read()
    assert "Statut    : Traitement terminé" in texte
    assert "entree.msp (2 Mo)" in texte
    assert "csv=1.0" in texte
    assert "2 spectres supprimés : pics vides" in texte

    lignes = lire_csv(dossier / noms[1])
    assert lignes[0][:3] == ["n", "etape", "tache"]
    assert lignes[1][1] == "Démarrage" and lignes[1][7] == "1,50"      # virgule décimale pour Excel en français
    parsing = [l for l in lignes if l[2] == "Parsing MSP"][0]
    assert parsing[1] == "Parsing" and parsing[3] == "3" and parsing[5] == "3"

    historique = lire_csv(dossier / "historique.csv")
    assert historique[0][0] == "date" and historique[1][1] == "essai" and historique[1][2] == "Traitement terminé"


def test_historique_complete_d_un_run_a_l_autre(tmp_path):
    for _ in range(2):
        perf_log.start({"output_directory": str(tmp_path)})
        perf_log.finish("terminé")
    assert len(lire_csv(tmp_path / "PERF_LOGS" / "historique.csv")) == 3   # en-tête + 2 runs


def test_dossier_impose_par_l_environnement(tmp_path, monkeypatch):
    monkeypatch.setenv("FRAGHUB_PERF_DIR", str(tmp_path / "journaux"))
    perf_log.start({"output_directory": str(tmp_path / "sortie")})
    txt = perf_log.finish("ok")
    assert os.path.dirname(txt) == str(tmp_path / "journaux")
    assert not (tmp_path / "sortie").exists()


def test_erreur_d_ecriture_ne_fait_pas_echouer_fraghub(tmp_path, monkeypatch, capsys):
    perf_log.start({"output_directory": str(tmp_path)})
    monkeypatch.setattr(perf_log, "_write", lambda s, status: 1 / 0)
    assert perf_log.finish() is None
    assert "Impossible d'écrire le journal" in capsys.readouterr().out


def test_valeur_avec_point_virgule_entre_guillemets_dans_le_csv(tmp_path):
    perf_log.start({"output_directory": str(tmp_path)})
    perf_log.on_prefix('Étape; avec "guillemets"')
    perf_log.on_total(1)
    perf_log.finish("ok")
    csv_path = [p for p in (tmp_path / "PERF_LOGS").iterdir() if p.name.startswith("perf_") and p.suffix == ".csv"][0]
    taches = [l[2] for l in lire_csv(csv_path)]
    assert 'Étape; avec "guillemets"' in taches
