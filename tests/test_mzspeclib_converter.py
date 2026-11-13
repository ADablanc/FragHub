# -*- coding: utf-8 -*-
"""Tests de scripts/mzspeclib_converter.py : traduction d'un spectre FragHub en mzSpecLib, et écriture des fichiers.

Le moteur Rust n'est pas utilisé : les spectres sont écrits à la main, comme ceux de la sortie JSON de FragHub.
"""
import json
import os

import pytest
from mzspeclib.backends.json import JSONSpectralLibraryWriter
from mzspeclib.backends.memory import InMemorySpectrumLibrary

import mzspeclib_converter as conv

CAFEINE = {
    "NAME": "Caffeine", "SPLASH": "splash10-0002-0900000000-1234", "PREDICTED": "false", "MSLEVEL": "2",
    "PRECURSORMZ": "195.0877", "PRECURSORTYPE": "[M+H]+", "IONMODE": "positive", "IONIZATION": "ESI",
    "FRAGMENTATIONMETHODE": "HCD", "COLLISIONENERGY": "20 eV", "RT": "3.25", "COMMENT": "known isomers: 0",
    "FORMULA": "C8H10N4O2", "SMILES": "Cn1cnc2c1c(=O)n(C)c(=O)n2C", "INCHIKEY": "RYYVLZVUVIJVGH-UHFFFAOYSA-N",
    "EXACTMASS": "194.08038", "SPECTRUMID": "CCMSLIB00000001", "NPCLASS_PATHWAY": "Alkaloids",
    "CLASSYFIRE_CLASS": "NOT FOUND", "AVERAGEMASS": "NaN",
    "PEAKS_LIST": [[138.0662, 100.0, "C7H8NO+/1.2"], [110.0713, 45.5]],
}


def payload(d, key=1):
    """Spectre traduit puis mis en forme, relu en dictionnaire."""
    text = conv.spectrum_text(conv.to_spectrum(d, key), JSONSpectralLibraryWriter(conv._NullHandle()))
    return json.loads(text)


def attrs(p):
    """{nom du terme: valeur} des attributs simples ; attributs libres sous la forme {"libre:CHAMP": valeur}."""
    out, groups = {}, {}
    for a in p["attributes"]:
        if "cv_param_group" in a:
            groups.setdefault(a["cv_param_group"], []).append(a)
        else:
            out[a["name"]] = a.get("value")
    for items in groups.values():
        names = {a["name"]: a for a in items}
        if "other attribute name" in names:
            out["libre:" + names["other attribute name"]["value"]] = names["other attribute value"]["value"]
        else:
            main = [a for a in items if a["name"] != "unit"][0]
            unit = [a for a in items if a["name"] == "unit"]
            out[main["name"]] = (main["value"], unit[0]["value"] if unit else None)
    return out


def analyte_attrs(p):
    return {a["name"]: a.get("value") for a in p["analytes"]["1"]["attributes"] if "cv_param_group" not in a}


# --- Petites fonctions ---

@pytest.mark.parametrize("adduct, charge", [
    ("[M+H]+", 1), ("[M-H]-", -1), ("[M+2H]2+", 2), ("[M-2H]2-", -2), ("[M]+*", 1), ("[M+Na]", None), ("", None),
])
def test_charge_lue_a_la_fin_de_l_adduit(adduct, charge):
    assert conv.adduct_charge(adduct) == charge


def test_valeurs_vides_non_ecrites():
    d = {"a": "", "b": "  NOT FOUND ", "c": "NaN", "d": None, "e": " 12 ", "f": 3.5}
    assert [conv._value(d, k) for k in "abcdef"] == [None, None, None, None, "12", "3.5"]
    assert conv._value(d, "absent") is None


def test_formule_de_novo_en_annotation_mzpaf():
    assert str(conv.denovo_annotation("C7H8NO+/1.23")[0]) == "f{C7H8NO}/1.23ppm"
    assert str(conv.denovo_annotation("C7H4NO3-/-0.5")[0]) == "f{C7H4NO3}/-0.5ppm"
    assert conv.denovo_annotation("pas une formule") == []
    assert conv.denovo_annotation("") == []


# --- Traduction d'un spectre complet ---

def test_spectre_complet_traduit_en_termes_psi_ms():
    p = payload(CAFEINE)
    a = attrs(p)
    assert a["library spectrum name"] == "Caffeine"
    assert a["splash key"] == "splash10-0002-0900000000-1234"
    assert a["spectrum origin type"] == "observed spectrum"
    assert a["ms level"] == 2
    assert a["experimental precursor monoisotopic m/z"] == (195.0877, "m/z")
    assert a["scan polarity"] == "positive scan"
    assert a["ionization type"] == "electrospray ionization"
    assert a["dissociation method"] == "beam-type collision-induced dissociation"
    assert a["collision energy"] == (20.0, "electronvolt")
    assert a["retention time"] == (3.25, "minute")
    assert a["library spectrum comment"] == "known isomers: 0"
    assert a["number of peaks"] == 2
    # Champs sans terme adapté : attributs libres qui gardent le nom FragHub
    assert a["libre:SPECTRUMID"] == "CCMSLIB00000001"
    assert a["libre:NPCLASS_PATHWAY"] == "Alkaloids"
    # Vides et « NOT FOUND » absents
    assert "libre:CLASSYFIRE_CLASS" not in a
    an = analyte_attrs(p)
    assert an["molecular formula"] == "C8H10N4O2"
    assert an["InChIKey"] == "RYYVLZVUVIJVGH-UHFFFAOYSA-N"
    assert an["adduct ion formula"] == "[M+H]+"
    assert an["charge state"] == 1
    # Pics : m/z, intensités, et formule de novo en annotation
    assert p["mzs"] == [138.0662, 110.0713] and p["intensities"] == [100.0, 45.5]
    assert p["peak_annotations"] == ["f{C7H8NO}/1.2ppm", "?"]


@pytest.mark.parametrize("energy, expected", [
    ("20 eV", ("collision energy", (20.0, "electronvolt"))),
    ("35 NCE", ("normalized collision energy", (35.0, "percent"))),
    ("Ramp 10/40 eV", ("collision energy ramp start", (10.0, "electronvolt"))),
    ("Ramp 20/60 NCE", ("percent collision energy ramp end", (60.0, "percent"))),
    ("40 V", ("libre:COLLISIONENERGY", "40 V")),       # unité refusée par le vocabulaire : gardée telle quelle
    ("10, 20, 40 eV", ("libre:COLLISIONENERGY", "10, 20, 40 eV")),
])
def test_energies_de_collision(energy, expected):
    a = attrs(payload({"COLLISIONENERGY": energy, "PEAKS_LIST": []}))
    assert a[expected[0]] == expected[1]


def test_valeurs_inconnues_gardees_en_attributs_libres():
    a = attrs(payload({"PREDICTED": "peut-être", "MSLEVEL": "2-3", "IONMODE": "both", "IONIZATION": "DESI",
                       "FRAGMENTATIONMETHODE": "SID", "PRECURSORMZ": "abc", "RT": "n/d", "PEAKS_LIST": []}))
    assert a["libre:PREDICTED"] == "peut-être"
    assert a["libre:MSLEVEL"] == "2-3"
    assert a["libre:IONMODE"] == "both"
    assert a["libre:IONIZATION"] == "DESI"
    assert a["libre:FRAGMENTATIONMETHODE"] == "SID"
    assert a["libre:PRECURSORMZ"] == "abc"
    assert a["libre:RT"] == "n/d"


def test_spectre_predit():
    assert attrs(payload({"PREDICTED": "true", "PEAKS_LIST": []}))["spectrum origin type"] == "predicted spectrum"


def test_valeur_libre_avec_barre_verticale_gardee_entiere():
    # mzspeclib coupait « Polyketides|Terpenoids » (NPClassifier) en accession + valeur
    a = attrs(payload({"NPCLASS_PATHWAY": "Polyketides|Terpenoids", "PEAKS_LIST": []}))
    assert a["libre:NPCLASS_PATHWAY"] == "Polyketides|Terpenoids"


def test_accession_de_terme_pas_recollee():
    p = payload({"IONMODE": "negative", "PEAKS_LIST": []})
    polarity = [x for x in p["attributes"] if x["name"] == "scan polarity"][0]
    assert polarity["value"] == "negative scan" and polarity["value_accession"] == "MS:1000129"


# --- Écriture des fichiers ---

def spectra(n, start=0):
    return [json.dumps({"NAME": f"Mol{i}", "PRECURSORMZ": "100.5", "IONMODE": "negative",
                        "PEAKS_LIST": [[50.0 + i, 100.0]]}) for i in range(start, start + n)]


@pytest.fixture
def sink(monkeypatch):
    """Réception sans processus parallèles (conversion dans le processus du test)."""
    s = conv.MzSpecLibSink(workers=1)
    monkeypatch.setattr(s, "_ensure_pool", lambda: setattr(s, "pool", False) or conv._worker_init())
    yield s
    s.close()


def reference_text(texts, name="POS_LC", version="2.0.0"):
    """Ce qu'écrirait mzspeclib pour toute la bibliothèque d'un coup (même en-tête, même mise en forme)."""
    library = InMemorySpectrumLibrary(create_index=False)
    library.add_attribute("MS:1003186|library format version", "1.0")
    library.add_attribute("MS:1003188|library name", name)
    library.add_attribute("MS:1003189|library description", f"Spectral library built by FragHub {version}")
    writer = JSONSpectralLibraryWriter(conv._NullHandle())
    writer.write_header(library)
    for key, text in enumerate(texts, 1):
        writer.write_spectrum(conv.to_spectrum(json.loads(text), key))
    return json.dumps(writer.buffer, indent=2, sort_keys=True)


def test_fichier_identique_a_celui_de_mzspeclib(tmp_path, sink):
    path = str(tmp_path / "MZ" / "POS_LC.mzSpecLib.json")
    texts = spectra(7)
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(texts[:3])   # plusieurs paquets
    sink.add(texts[3:])
    sink.end()
    assert open(path, encoding="ascii").read() == reference_text(texts)
    assert sorted(os.listdir(tmp_path / "MZ")) == ["POS_LC.mzSpecLib.json"]   # ni .part ni .tmp


def test_bibliotheque_sans_spectre(tmp_path, sink):
    path = str(tmp_path / "NEG_GC.mzSpecLib.json")
    sink.begin(path, "NEG_GC", version="2.0.0")
    sink.end()
    data = json.load(open(path, encoding="ascii"))
    assert data["spectra"] == [] and data["attributes"][1]["value"] == "NEG_GC"


def test_mise_a_jour_ajoute_a_la_suite_avec_les_cles_suivantes(tmp_path, sink):
    path = str(tmp_path / "POS_LC.mzSpecLib.json")
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(spectra(3))
    sink.end()
    sink.begin(path, "POS_LC", update=True, version="2.0.0")
    sink.add(spectra(2, start=3))
    sink.end()
    # même résultat que les 5 spectres écrits d'un coup
    assert open(path, encoding="ascii").read() == reference_text(spectra(5))


def test_mise_a_jour_sans_nouveau_spectre_fichier_inchange(tmp_path, sink):
    path = str(tmp_path / "POS_LC.mzSpecLib.json")
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(spectra(2))
    sink.end()
    before = open(path, "rb").read()
    sink.begin(path, "POS_LC", update=True, version="2.0.0")
    sink.end()
    assert open(path, "rb").read() == before


def test_spectre_fautif_ignore_les_autres_ecrits(tmp_path, sink, capsys):
    path = str(tmp_path / "POS_LC.mzSpecLib.json")
    texts = spectra(3)
    texts[1] = json.dumps({"NAME": "cassé", "PEAKS_LIST": [["pas un nombre", 1]]})
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(texts)
    sink.end()
    data = json.load(open(path, encoding="ascii"))
    names = [a["value"] for s in data["spectra"] for a in s["attributes"] if a["name"] == "library spectrum name"]
    assert names == ["Mol0", "Mol2"]
    assert "1 spectre(s) ignoré(s)" in capsys.readouterr().out


def test_arret_ne_laisse_rien_et_garde_l_ancien_fichier(tmp_path, sink):
    path = str(tmp_path / "POS_LC.mzSpecLib.json")
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(spectra(2))
    sink.end()
    before = open(path, "rb").read()
    sink.begin(path, "POS_LC", version="2.0.0")
    sink.add(spectra(5))
    sink.abort()
    assert open(path, "rb").read() == before
    assert sorted(os.listdir(tmp_path)) == ["POS_LC.mzSpecLib.json"]


def test_conversion_parallele_meme_resultat(tmp_path):
    """Avec de vrais processus de conversion : même fichier qu'en une seule fois."""
    path = str(tmp_path / "POS_LC.mzSpecLib.json")
    texts = spectra(30)
    s = conv.MzSpecLibSink(workers=2)
    try:
        s.begin(path, "POS_LC", version="2.0.0")
        for i in range(0, 30, 4):
            s.add(texts[i:i + 4])
        s.end()
    finally:
        s.close()
    assert open(path, encoding="ascii").read() == reference_text(texts)
