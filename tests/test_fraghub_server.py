# -*- coding: utf-8 -*-
"""Tests de scripts/FragHub.py : adresses du serveur (API utilisée par l'interface) et messages de progression.

Le moteur Rust (fraghub_rust) est importé par FragHub.py mais n'est jamais lancé ici : ses fonctions sont remplacées
le temps de chaque test (monkeypatch). La base interne est remplacée de même : aucun téléchargement.
"""
import pytest

pytest.importorskip("fraghub_rust", reason="module Rust non compilé (python build_backend.py --rust-only)")
from fastapi.testclient import TestClient  # noqa: E402

import FragHub as fh  # noqa: E402

PARAMS = {
    "input_directory": ["a.msp"], "output_directory": "sortie", "normalize_intensity": 1.0,
    "remove_peak_above_precursormz": 1.0, "check_minimum_peak_requiered": 1.0,
    "check_minimum_peak_requiered_n_peaks": 3.0, "reduce_peak_list": 1.0, "reduce_peak_list_max_peaks": 500.0,
    "remove_spectrum_under_entropy_score": 1.0, "remove_spectrum_under_entropy_score_value": 0.5,
    "keep_mz_in_range": 1.0, "keep_mz_in_range_from_mz": 50.0, "keep_mz_in_range_to_mz": 2000.0,
    "check_minimum_of_high_peaks_requiered": 1.0, "check_minimum_of_high_peaks_requiered_intensity_percent": 5.0,
    "check_minimum_of_high_peaks_requiered_no_peaks": 2.0, "calculate_de_novo": 0.0, "de_novo_ppm_tolerance": 10.0,
    "csv": 1.0, "msp": 1.0, "json": 1.0, "reset_updates": 0.0,
}


@pytest.fixture
def client():
    return TestClient(fh.app)


@pytest.fixture
def messages(monkeypatch):
    """Messages envoyés à l'interface, gardés dans une liste au lieu de partir par Socket.IO."""
    sent = []
    monkeypatch.setattr(fh, "emit_to_frontend", lambda event, data: sent.append((event, data)))
    monkeypatch.setattr(fh, "task_state", {"task": 0, "total": 0, "prefix": "", "item_type": "items", "started": 0})
    monkeypatch.setattr(fh, "global_stop_flag", False)
    return sent


@pytest.fixture
def statut_base(monkeypatch):
    """État de la base remis à « idle » ; renvoie une fonction qui fixe ce que local_state répondra."""
    monkeypatch.setattr(fh, "database_status", {"state": "idle", "percent": 0.0, "message": ""})
    monkeypatch.setattr(fh.internal_database, "read_reference", lambda base: {"version": "x"})
    started = []
    monkeypatch.setattr(fh, "_prepare_database", lambda: started.append(True))

    def set_state(state):
        monkeypatch.setattr(fh.internal_database, "local_state", lambda base, ref: state)
        return started
    return set_state


# --- Adresses simples ---

def test_health(client):
    assert client.get("/health").json() == {"status": "ok"}


def test_rapport_absent(client, tmp_path):
    assert client.get("/report", params={"path": str(tmp_path / "absent.html")}).json() == {"error": "Report not found"}


def test_rapport_servi(client, tmp_path):
    report = tmp_path / "report.html"
    report.write_text("<html>rapport</html>", encoding="utf-8")
    r = client.get("/report", params={"path": str(report)})
    assert r.status_code == 200 and r.text == "<html>rapport</html>"


def test_stop_leve_l_indicateur_et_previent_le_moteur(client, monkeypatch, messages):
    called = []
    monkeypatch.setattr(fh.fraghub_rust, "request_stop", lambda: called.append(True))
    assert client.get("/stop-analysis").json() == {"status": "stopped"}
    assert fh.global_stop_flag is True and called == [True]


# --- Base interne au démarrage (écran de démarrage) ---

def test_base_presente_prete_tout_de_suite(client, statut_base):
    started = statut_base("reference")
    assert client.get("/database-status").json()["state"] == "ready"
    assert started == []


def test_base_absente_telechargement_lance_une_seule_fois(client, statut_base):
    started = statut_base("absente")
    assert client.get("/database-status").json()["state"] == "downloading"
    assert client.get("/database-status").json()["state"] == "downloading"
    assert started == [True]   # un seul téléchargement, même si l'écran interroge plusieurs fois


def test_reference_illisible_erreur(client, statut_base, monkeypatch):
    statut_base("absente")

    def broken(base):
        raise fh.internal_database.InternalDatabaseError("illisible")
    monkeypatch.setattr(fh.internal_database, "read_reference", broken)
    body = client.get("/database-status").json()
    assert body["state"] == "error" and "reinstall" in body["message"]


def test_init_data_refuse_pendant_le_telechargement(client, monkeypatch):
    monkeypatch.setattr(fh, "database_status", {"state": "downloading", "percent": 10.0, "message": ""})
    assert client.get("/init-data").status_code == 503


def test_init_data_charge_la_base(client, monkeypatch):
    monkeypatch.setattr(fh, "database_status", {"state": "ready", "percent": 100.0, "message": ""})
    monkeypatch.setattr(fh.internal_database, "ensure_internal_database", lambda base, **k: "reference")
    loaded = []
    monkeypatch.setattr(fh.fraghub_rust, "load_internal_databases", lambda base: loaded.append(base))
    assert client.get("/init-data").json() == {"status": "loaded"}
    assert loaded == [fh.BASE_DIR]


def test_init_data_echec_du_telechargement(client, monkeypatch):
    monkeypatch.setattr(fh, "database_status", {"state": "ready", "percent": 100.0, "message": ""})

    def fail(base, **k):
        raise fh.internal_database.InternalDatabaseError("pas de connexion")
    monkeypatch.setattr(fh.internal_database, "ensure_internal_database", fail)
    r = client.get("/init-data")
    assert r.status_code == 503 and "pas de connexion" in r.json()["detail"]


def test_preparation_de_la_base_en_arriere_plan(monkeypatch):
    monkeypatch.setattr(fh, "database_status", {"state": "downloading", "percent": 0.0, "message": ""})

    def fake_ensure(base, progress=None):
        progress("download", 500_000_000, 1_000_000_000)
        assert fh.database_status["message"] == "Downloading molecule database… 50 % of 1.0 GB"
        progress("verify", 1, 1)
        progress("extract", 1, 1)
        return "telechargee"
    monkeypatch.setattr(fh.internal_database, "ensure_internal_database", fake_ensure)
    fh._prepare_database()
    assert fh.database_status["state"] == "ready"

    def fail(base, progress=None):
        raise fh.internal_database.InternalDatabaseError("coupure")
    monkeypatch.setattr(fh.internal_database, "ensure_internal_database", fail)
    fh._prepare_database()
    assert fh.database_status["state"] == "error" and "restart" in fh.database_status["message"]


# --- Lancement d'un run ---

def test_run_parametres_incomplets_refuses(client):
    params = dict(PARAMS)
    del params["output_directory"]
    assert client.post("/run-analysis", json=params).status_code == 422


def test_run_lance_en_arriere_plan_avec_les_parametres(client, monkeypatch):
    launched = []
    monkeypatch.setattr(fh, "execute_main_safely_with_perf_log", lambda: launched.append(dict(fh.parameters_dict)))
    monkeypatch.setattr(fh, "global_stop_flag", True)   # reste d'un run arrêté
    assert client.post("/run-analysis", json=PARAMS).json() == {"status": "started"}
    p = launched[0]
    assert p["json"] == 1.0 and p["mzspeclib_json"] == 1.0 and p["write_perf_logs"] == 0.0   # valeurs par défaut
    assert p["input_db_names"] == {} and p["output_directory"] == "sortie"
    assert fh.global_stop_flag is False


# --- Messages de progression vers l'interface ---

def test_chaque_progression_porte_l_etat_de_la_tache(messages):
    fh.prefix_callback("Parsing MSP:")
    fh.item_type_callback("spectra")
    fh.total_items_callback(200)
    fh.progress_callback(50)
    events = [e for e, _ in messages]
    assert events == ["prefix", "item_type", "total_items", "progress"]
    progress = messages[-1][1]
    assert progress["value"] == 50 and progress["task"] == 1 and progress["total"] == 200
    assert progress["prefix"] == "Parsing MSP:" and progress["item_type"] == "spectra"
    fh.total_items_callback(10)
    assert messages[-1][1]["task"] == 2   # nouvelle tâche : numéro suivant


def test_progression_arretee_par_l_utilisateur(messages, monkeypatch):
    monkeypatch.setattr(fh, "global_stop_flag", True)
    with pytest.raises(Exception, match="stopped by user"):
        fh.progress_callback(1)


def test_fin_de_run_transmise_avec_le_rapport(messages, monkeypatch):
    monkeypatch.setattr(fh, "parameters_dict", {"mzspeclib_json": 0.0})

    def fake_orchestrator(params, progress, total, prefix, item_type, step, completion, deletion, stop, mzspeclib_sink=None):
        step("Parsing")
        completion("Process completed", "sortie/report.html")
    monkeypatch.setattr(fh.fraghub_rust, "main_orchestrator", fake_orchestrator)
    fh.execute_main_safely()
    assert ("completion", {"message": "Process completed", "report_path": "sortie/report.html"}) in messages


def test_arret_pendant_le_run(messages, monkeypatch):
    monkeypatch.setattr(fh, "parameters_dict", {"mzspeclib_json": 0.0})

    def stopped(*a, **k):
        raise Exception("Process stopped by user.")
    monkeypatch.setattr(fh.fraghub_rust, "main_orchestrator", stopped)
    fh.execute_main_safely()
    assert ("completion", {"message": "Process stopped by user.", "report_path": None}) in messages
    assert any(e == "deletion" and "INTERRUPTED" in d for e, d in messages)


def test_erreur_du_moteur_transmise(messages, monkeypatch, capsys):
    monkeypatch.setattr(fh, "parameters_dict", {"mzspeclib_json": 0.0})

    def broken(*a, **k):
        raise ValueError("fichier illisible")
    monkeypatch.setattr(fh.fraghub_rust, "main_orchestrator", broken)
    fh.execute_main_safely()
    assert ("completion", {"message": "Error: fichier illisible", "report_path": None}) in messages


def test_sortie_mzspeclib_toujours_fermee(messages, monkeypatch):
    monkeypatch.setattr(fh, "parameters_dict", {"mzspeclib_json": 1.0})
    closed = []

    class FakeSink:
        def close(self, terminate=False):
            closed.append(terminate)
    monkeypatch.setattr(fh.mzspeclib_converter, "MzSpecLibSink", FakeSink)

    def broken(*a, mzspeclib_sink=None, **k):
        assert isinstance(mzspeclib_sink, FakeSink)
        raise ValueError("erreur")
    monkeypatch.setattr(fh.fraghub_rust, "main_orchestrator", broken)
    fh.execute_main_safely()
    assert closed == [True]   # processus de conversion arrêtés même après une erreur


def test_journal_de_performances_seulement_si_demande(messages, monkeypatch):
    calls = []
    monkeypatch.setattr(fh, "execute_main_safely", lambda: calls.append("run"))
    monkeypatch.setattr(fh.perf_log, "start", lambda p: calls.append("start"))
    monkeypatch.setattr(fh.perf_log, "finish", lambda: calls.append("finish"))
    monkeypatch.setattr(fh, "parameters_dict", {"write_perf_logs": 0.0})
    fh.execute_main_safely_with_perf_log()
    assert calls == ["run"]
    calls.clear()
    monkeypatch.setattr(fh, "parameters_dict", {"write_perf_logs": 1.0})
    fh.execute_main_safely_with_perf_log()
    assert calls == ["start", "run", "finish"]
