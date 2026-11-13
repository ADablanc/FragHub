# -*- coding: utf-8 -*-
"""Tests de sync_version.py (numéro de version recopié dans tout le projet) et de petites fonctions de build_utils.py."""
import shutil

import pytest

import build_utils
import sync_version


def test_version_du_depot_identique_partout():
    """Les fichiers du dépôt donnent tous la version de pyproject.toml, et chaque motif trouve son nombre exact
    d'occurrences (un motif cassé par une modification d'un fichier serait signalé ici)."""
    version = sync_version.read_source_version()
    changed, problems = sync_version.sync(version, check_only=True, quiet=True)
    assert problems == [] and changed == []


@pytest.fixture
def copie_du_projet(tmp_path, monkeypatch):
    """Copie des seuls fichiers concernés, dans un dossier temporaire : sync_version y travaille au lieu du dépôt."""
    files = {"pyproject.toml"} | {rel for rel, _, _ in sync_version.TARGETS}
    for rel in files:
        dst = tmp_path / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(sync_version.ROOT / rel, dst)
    monkeypatch.setattr(sync_version, "ROOT", tmp_path)
    monkeypatch.setattr(sync_version, "SOURCE", tmp_path / "pyproject.toml")
    return tmp_path


def test_nouvelle_version_recopiee_partout(copie_du_projet):
    before = {rel: (copie_du_projet / rel).read_bytes() for rel, _, _ in sync_version.TARGETS}
    sync_version.write_source_version("9.8.7")
    assert sync_version.read_source_version() == "9.8.7"
    changed, problems = sync_version.sync("9.8.7", quiet=True)
    assert problems == [] and sorted(changed) == sorted({rel for rel, _, _ in sync_version.TARGETS})
    for rel, old in before.items():
        new = (copie_du_projet / rel).read_bytes()
        assert b"9.8.7" in new
        assert new.count(b"\r\n") == old.count(b"\r\n")   # fins de ligne d'origine gardées (CRLF sous Windows)
    # ensuite tout est à jour
    assert sync_version.sync("9.8.7", check_only=True, quiet=True) == ([], [])


def test_verification_seule_n_ecrit_rien(copie_du_projet):
    before = {p: p.read_bytes() for p in copie_du_projet.rglob("*") if p.is_file()}
    changed, problems = sync_version.sync("9.8.7", check_only=True, quiet=True)
    assert changed and problems == []
    assert {p: p.read_bytes() for p in copie_du_projet.rglob("*") if p.is_file()} == before


def test_motif_qui_ne_trouve_plus_rien_signale(copie_du_projet):
    readme = copie_du_projet / "README.md"
    readme.write_text("# Autre titre\n", encoding="utf-8")
    changed, problems = sync_version.sync("2.0.0", check_only=True, quiet=True)
    assert any(p.startswith("README.md : 0 occurrence") for p in problems)


@pytest.mark.parametrize("seconds, text", [(5, "5 s"), (59.9, "59 s"), (60, "1 min 00 s"), (3725, "62 min 05 s")])
def test_duree_lisible(seconds, text):
    assert build_utils.format_duration(seconds) == text


def test_executables_du_venv():
    exe = build_utils.venv_bin("python")
    assert exe.parent.parent == build_utils.VENV_DIR
    assert exe.name == ("python.exe" if build_utils.IS_WINDOWS else "python")
