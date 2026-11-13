# -*- coding: utf-8 -*-
"""Tests de scripts/build_dataset/database_metrics.py (métriques et sunbursts de la base interne, pour le README)."""
import json
import os
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "build_dataset"))

pa = pytest.importorskip("pyarrow", reason="pyarrow sert à lire la base (uv sync)")
import pyarrow.parquet as pq  # noqa: E402

import database_metrics as dm  # noqa: E402

CF = ["CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS"]
NP = ["NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS"]


def row(key, cf, np_, structure=True):
    r = {"INCHIKEY": key, "INCHI": "InChI=1S/x" if structure else None, "SMILES": "C" if structure else None,
         "FORMULA": "C" if structure else None, "NAME": "n", "EXACTMASS": 12.0 if structure else None}
    r.update(dict(zip(CF, cf)))
    r.update(dict(zip(NP, np_)))
    return r


@pytest.fixture
def base(tmp_path):
    rows = [
        row("A", ("Lipids", "Fatty Acyls", "Fatty acids"), ("Fatty acids", "Fatty acyls", "Wax")),
        row("B", ("Lipids", "Fatty Acyls", "NOT FOUND"), ("Alkaloids|Terpenoids", "NOT FOUND", "NOT FOUND")),
        row("C", ("Lipids", "Prenol lipids", None), ("NOT FOUND", "NOT FOUND", "NOT FOUND")),
        row("D", (None, None, None), ("Fatty acids", "Fatty acyls", "Wax"), structure=False),
    ]
    folder = tmp_path / "internal_databases.parquet"
    folder.mkdir()
    pq.write_table(pa.Table.from_pylist(rows[:2]), folder / "part_0000.parquet")
    pq.write_table(pa.Table.from_pylist(rows[2:]), folder / "part_0001.parquet")
    return str(folder)


def test_comptes(base):
    total, filled, paths = dm.count(base)
    assert total == 4
    assert filled["INCHI"] == 3 and filled["NAME"] == 4
    assert paths["classyfire"][("Lipids", "Fatty Acyls", "Unknown")] == 1      # « NOT FOUND » = inconnu
    assert paths["classyfire"][("Unknown", "Unknown", "Unknown")] == 1         # vide = inconnu
    assert paths["npclassifier"][("Alkaloids | Terpenoids", "Unknown", "Unknown")] == 1
    s = dm.summary(total, filled, paths)
    assert [lv["classified"] for lv in s["ontologies"]["classyfire"]["levels"]] == [3, 3, 1]
    assert [lv["classes"] for lv in s["ontologies"]["npclassifier"]["levels"]] == [2, 1, 1]


def test_sunburst_avec_et_sans_inconnus(base):
    _, _, paths = dm.count(base)
    full = dm.sunburst(paths["classyfire"], True)
    values = dict(zip(full["ids"], full["values"]))
    assert values["Lipids"] == 3 and values["Unknown"] == 1
    assert values["Lipids / Fatty Acyls / Unknown"] == 1
    # Chaque case vaut la somme de ses enfants quand tout est gardé
    for i, parent in zip(full["ids"], full["parents"]):
        children = [v for c, p, v in zip(full["ids"], full["parents"], full["values"]) if p == i]
        if children:
            assert sum(children) == values[i]
    assert full["colors"][full["ids"].index("Unknown")] == dm.UNKNOWN_COLOR

    known = dm.sunburst(paths["classyfire"], False)
    kvalues = dict(zip(known["ids"], known["values"]))
    assert not any("Unknown" in i for i in known["ids"])
    # Case gardée entière : « Fatty Acyls » vaut 2 alors que seul un enfant est classé
    assert kvalues == {"Lipids": 3, "Lipids / Fatty Acyls": 2, "Lipids / Prenol lipids": 1,
                       "Lipids / Fatty Acyls / Fatty acids": 1}
    assert set(known["parents"]) <= set(known["ids"]) | {""}


def test_page_et_fichiers(base, tmp_path, monkeypatch):
    out = tmp_path / "doc"
    monkeypatch.setattr(dm, "OUT", str(out))
    root = tmp_path / "racine"
    (root / "datas").mkdir(parents=True)
    (root / "datas" / "internal_database.json").write_text(json.dumps({"version": "2099-01-01", "doi": "10.1/x"}))
    monkeypatch.setattr(dm, "ROOT", str(root))
    monkeypatch.setattr("sys.argv", ["database_metrics.py", "--base", base, "--no-png"])
    dm.main()
    meta = json.loads((out / "metrics.json").read_text(encoding="utf-8"))
    assert meta["version"] == "2099-01-01" and meta["summary"]["molecules"] == 4
    page = (out / "internal_database_metrics.html").read_text(encoding="utf-8")
    assert "2099-01-01" in page and "doi.org/10.1/x" in page and dm.PLOTLY in page
    charts = json.loads(page.split("const CHARTS = ")[1].split(";\n")[0])
    assert set(charts) == {"classyfire", "classyfire_known", "npclassifier", "npclassifier_known"}
    assert not list(out.glob("*.png"))
