# -*- coding: utf-8 -*-
"""Tests de scripts/internal_database.py : état de la base sur le disque et téléchargement depuis Zenodo.

Le téléchargement est testé avec un petit serveur HTTP local (voir conftest.py) et une fausse base de trois
fichiers : rien ne passe par Internet, rien n'est écrit ailleurs que dans le dossier temporaire du test.
"""
import hashlib
import io
import json
import os
import zipfile

import pytest

import internal_database as idb

FILES = {"part_0000.parquet": b"A" * 1000, "part_0001.parquet": b"B" * 2000, "part_0002.parquet": b"C" * 10}


def make_archive(files=FILES):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as z:
        for name, data in files.items():
            z.writestr(name, data)
    return buf.getvalue()


def write_reference(root, archive, url, files=FILES, **changes):
    """Écrit datas/internal_database.json dans un faux dépôt `root`, comme make_archive.py."""
    ref = {"version": "2099-01-01", "doi": "10.5281/zenodo.1", "url": url, "size": len(archive),
           "sha256": hashlib.sha256(archive).hexdigest(),
           "files": [{"name": n, "size": len(d), "sha256": hashlib.sha256(d).hexdigest()} for n, d in files.items()]}
    ref.update(changes)
    os.makedirs(root / "datas", exist_ok=True)
    (root / "datas" / "internal_database.json").write_text(json.dumps(ref), encoding="utf-8")
    return ref


def db_dir(root):
    return root / "datas" / "internal_databases.parquet"


def install(root, files=FILES):
    os.makedirs(db_dir(root), exist_ok=True)
    for name, data in files.items():
        (db_dir(root) / name).write_bytes(data)


@pytest.fixture(autouse=True)
def no_wait(monkeypatch):
    # Entre deux essais de téléchargement, le module attend quelques secondes : inutile ici
    monkeypatch.setattr(idb.time, "sleep", lambda s: None)


# --- Fichier de référence ---

def test_reference_lue(tmp_path):
    ref = write_reference(tmp_path, b"zip", "http://x/a.zip")
    assert idb.read_reference(str(tmp_path)) == ref


def test_reference_sans_champ_obligatoire(tmp_path):
    ref = write_reference(tmp_path, b"zip", "http://x/a.zip")
    del ref["sha256"]
    (tmp_path / "datas" / "internal_database.json").write_text(json.dumps(ref), encoding="utf-8")
    with pytest.raises(idb.InternalDatabaseError, match="sha256"):
        idb.read_reference(str(tmp_path))


def test_reference_illisible(tmp_path):
    os.makedirs(tmp_path / "datas")
    (tmp_path / "datas" / "internal_database.json").write_text("{pas du json", encoding="utf-8")
    with pytest.raises(idb.InternalDatabaseError, match="illisible"):
        idb.read_reference(str(tmp_path))


# --- État de la base sur le disque ---

def test_etat_absente_reference_differente(tmp_path):
    ref = write_reference(tmp_path, make_archive(), "http://x/a.zip")
    assert idb.local_state(str(tmp_path), ref) == "absente"
    os.makedirs(db_dir(tmp_path))
    assert idb.local_state(str(tmp_path), ref) == "absente"          # dossier vide
    install(tmp_path)
    assert idb.local_state(str(tmp_path), ref) == "reference"
    (db_dir(tmp_path) / "part_0002.parquet").write_bytes(b"C" * 11)   # une taille différente
    assert idb.local_state(str(tmp_path), ref) == "differente"


def test_etat_fichier_en_trop(tmp_path):
    ref = write_reference(tmp_path, make_archive(), "http://x/a.zip")
    install(tmp_path, {**FILES, "part_0003.parquet": b"D"})
    assert idb.local_state(str(tmp_path), ref) == "differente"


def test_etat_un_seul_fichier_parquet(tmp_path):
    ref = write_reference(tmp_path, make_archive(), "http://x/a.zip")
    db_dir(tmp_path).write_bytes(b"un parquet unique")   # accepté par le moteur à la place du dossier
    assert idb.local_state(str(tmp_path), ref) == "differente"


# --- Base présente : jamais retéléchargée ni modifiée ---

def test_base_de_reference_rien_a_faire(tmp_path, file_server):
    serve, url, handler = file_server
    write_reference(tmp_path, make_archive(), url)
    install(tmp_path)
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "reference"
    assert handler.requests == []


def test_base_differente_gardee_telle_quelle(tmp_path, file_server):
    serve, url, handler = file_server
    write_reference(tmp_path, make_archive(), url)
    install(tmp_path, {"part_0000.parquet": "base recalculée".encode()})
    messages = []
    assert idb.ensure_internal_database(str(tmp_path), log=messages.append) == "differente"
    assert handler.requests == []
    assert (db_dir(tmp_path) / "part_0000.parquet").read_bytes() == "base recalculée".encode()
    assert "utilisés tels quels" in messages[0]


# --- Version précédente publiée : remplacée par celle de la référence ---

OLD = {"part_0000.parquet": b"a" * 900, "part_0001.parquet": b"b" * 1900}   # version précédente (autres tailles)


def previous(files=OLD, version="2098-01-01"):
    return [{"version": version, "doi": "10.5281/zenodo.0", "files": [{"name": n, "size": len(d)} for n, d in files.items()]}]


def test_etat_ancienne_version_publiee(tmp_path):
    ref = write_reference(tmp_path, make_archive(), "http://x/a.zip", previous_versions=previous())
    install(tmp_path, OLD)
    assert idb.local_state(str(tmp_path), ref) == "ancienne"
    assert idb.installed_previous_version(str(tmp_path), ref) == "2098-01-01"
    (db_dir(tmp_path) / "part_0000.parquet").write_bytes(b"a" * 901)   # retouchée : plus une version publiée
    assert idb.local_state(str(tmp_path), ref) == "differente"


def test_ancienne_version_remplacee(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive)
    write_reference(tmp_path, archive, url, previous_versions=previous())
    install(tmp_path, OLD)
    messages = []
    assert idb.ensure_internal_database(str(tmp_path), log=messages.append) == "mise_a_jour"
    # Nouvelle base seule (l'ancienne ne laisse aucun fichier), rien d'autre dans datas/
    assert sorted(os.listdir(db_dir(tmp_path))) == sorted(FILES)
    for name, data in FILES.items():
        assert (db_dir(tmp_path) / name).read_bytes() == data
    assert sorted(os.listdir(tmp_path / "datas")) == ["internal_database.json", "internal_databases.parquet"]
    assert "2098-01-01" in messages[0] and "remplacée" in messages[0]
    # Au démarrage suivant : rien à faire
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "reference"
    assert len(handler.requests) == 1


def test_mise_a_jour_impossible_ancienne_base_gardee(tmp_path):
    archive = make_archive()
    write_reference(tmp_path, archive, "http://127.0.0.1:9/archive.zip", previous_versions=previous())   # injoignable
    install(tmp_path, OLD)
    messages = []
    assert idb.ensure_internal_database(str(tmp_path), log=messages.append) == "ancienne"
    for name, data in OLD.items():
        assert (db_dir(tmp_path) / name).read_bytes() == data
    assert "en attendant" in messages[-1]


def test_base_recalculee_jamais_remplacee_meme_avec_versions_precedentes(tmp_path, file_server):
    serve, url, handler = file_server
    write_reference(tmp_path, make_archive(), url, previous_versions=previous())
    install(tmp_path, {"part_0000.parquet": "base recalculée".encode()})
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "differente"
    assert handler.requests == []


# --- Téléchargement ---

def test_telechargement_complet(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive)
    write_reference(tmp_path, archive, url)
    steps = []
    state = idb.ensure_internal_database(str(tmp_path), log=lambda m: None, progress=lambda s, d, t: steps.append(s))
    assert state == "telechargee"
    for name, data in FILES.items():
        assert (db_dir(tmp_path) / name).read_bytes() == data
    # ni archive partielle ni dossier provisoire laissés derrière
    assert sorted(os.listdir(tmp_path / "datas")) == ["internal_database.json", "internal_databases.parquet"]
    assert steps[0] == "download" and steps[-2:] == ["verify", "extract"]


def test_reprise_apres_coupure(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive)
    ref = write_reference(tmp_path, archive, url)
    part = tmp_path / "datas" / ("internal_database_%s.zip.part" % ref["version"])
    part.write_bytes(archive[:1500])   # téléchargement interrompu
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "telechargee"
    assert handler.requests == ["bytes=1500-"]
    assert (db_dir(tmp_path) / "part_0001.parquet").read_bytes() == FILES["part_0001.parquet"]


def test_serveur_sans_reprise_repart_du_debut(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive, accept_range=False)   # renvoie tout le fichier (200) malgré la demande de reprise
    ref = write_reference(tmp_path, archive, url)
    (tmp_path / "datas" / ("internal_database_%s.zip.part" % ref["version"])).write_bytes(b"debut abime")
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "telechargee"
    assert (db_dir(tmp_path) / "part_0000.parquet").read_bytes() == FILES["part_0000.parquet"]


def test_archive_corrompue_refusee_et_supprimee(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive)
    ref = write_reference(tmp_path, archive, url, sha256="0" * 64)
    with pytest.raises(idb.InternalDatabaseError, match="SHA-256"):
        idb.ensure_internal_database(str(tmp_path), log=lambda m: None)
    assert not db_dir(tmp_path).exists()
    assert not (tmp_path / "datas" / ("internal_database_%s.zip.part" % ref["version"])).exists()


def test_archive_avec_d_autres_fichiers_refusee(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive({"autre.parquet": b"X"})
    serve(archive)
    write_reference(tmp_path, archive, url)   # la référence annonce part_0000..0002
    with pytest.raises(idb.InternalDatabaseError, match="fichiers annoncés"):
        idb.ensure_internal_database(str(tmp_path), log=lambda m: None)
    assert not db_dir(tmp_path).exists()


def test_serveur_injoignable_message_avec_lien(tmp_path):
    archive = make_archive()
    write_reference(tmp_path, archive, "http://127.0.0.1:9/archive.zip")   # port 9 : personne n'écoute
    with pytest.raises(idb.InternalDatabaseError) as e:
        idb.ensure_internal_database(str(tmp_path), log=lambda m: None)
    message = str(e.value)
    assert "https://doi.org/10.5281/zenodo.1" in message and "reprendra" in message
    assert not db_dir(tmp_path).exists()


def test_pas_assez_de_place(tmp_path, monkeypatch):
    write_reference(tmp_path, make_archive(), "http://x/a.zip", size=10 ** 12)
    monkeypatch.setattr(idb.shutil, "disk_usage", lambda p: type("U", (), {"free": 10 ** 9})())
    with pytest.raises(idb.InternalDatabaseError, match="pas assez de place"):
        idb.ensure_internal_database(str(tmp_path), log=lambda m: None)


def test_dossier_vide_remplace_par_la_base(tmp_path, file_server):
    serve, url, handler = file_server
    archive = make_archive()
    serve(archive)
    write_reference(tmp_path, archive, url)
    os.makedirs(db_dir(tmp_path))   # dossier présent mais sans .parquet
    assert idb.ensure_internal_database(str(tmp_path), log=lambda m: None) == "telechargee"
    assert sorted(os.listdir(db_dir(tmp_path))) == sorted(FILES)
