# -*- coding: utf-8 -*-
"""Tests des textes des barres de progression de la ligne de commande (scripts/progress_format.py) et du nom de
base deviné d'après le nom du fichier (scripts/database_name.py), mêmes règles que l'interface."""
import pytest

import database_name
import progress_format as pf


def test_tailles_en_mb_puis_en_gb():
    assert pf.size_unit(999_999_999) == "MB" and pf.size_unit(1_000_000_000) == "GB"
    assert pf.format_size(350_200_000) == "350.2 MB"
    assert pf.format_size(1_420_000_000) == "1.42 GB"
    assert pf.format_size(2_048_000_000, "MB") == "2,048.0 MB"
    assert pf.format_rate(1_350_000_000) == "1.35 GB/s"


def test_lecture_en_octets():
    assert pf.progress_texts(350_200_000, 800_000_000, "bytes", 2) == ("350.2 of 800.0 MB", "175.1 MB/s")
    assert pf.progress_texts(350_200_000, 2_048_000_000, "bytes", 2)[0] == "0.35 of 2.05 GB"
    assert pf.progress_texts(0, 10, "bytes", 0) == ("0.0 of 0.0 MB", "0.0 MB/s")


def test_ecriture_spectres_et_octets():
    assert pf.progress_texts(1200, 5000, "spectra", 4, 1_420_000_000) == \
        ("1200 of 5000 spectra | 1.42 GB written", "355.0 MB/s")


def test_autres_etapes_en_elements():
    assert pf.progress_texts(30, 300, "rows", 2) == ("30 of 300 rows", "15.0 rows/s")


@pytest.mark.parametrize("fichier, base", [
    ("20241003_enamdisc_neg_ms2.mgf", "MSnLib"),
    ("20250828_targetmol_hts_np_pos_ms2.mgf", "MSnLib"),
    ("ALL_GNPS.json", "GNPS"),
    ("CCMSLIB00000001547.mgf", "GNPS"),
    ("MassBank_RIKENformat.msp", "MassBank"),
    ("MoNA-export-Experimental_Spectra.msp", "MoNA"),
    ("MSMS-Public_experimentspectra-pos-VS19.msp", "MS-DIAL"),
    ("spectraverse-1.0.2.mgf", "Spectraverse"),
    ("nist23_hr_msms.msp", "NIST"),
    (r"C:\GNPS\data\MoNA.msp", "MoNA"),              # seul le nom du fichier compte
    ("/home/axel/MoNA/mes_spectres.mgf", "Unknown"),
    ("monacolin_standards.mgf", "Unknown"),          # « mona » pris dans un autre mot
    ("irrespective.msp", "Unknown"),
    ("", "Unknown"),
    (None, "Unknown"),
])
def test_nom_de_base_devine(fichier, base):
    assert database_name.guess_database_name(fichier) == base
