# -*- coding: utf-8 -*-
"""
Nom de la base d'origine d'un fichier d'entrée, deviné d'après le nom du fichier : utilisé en ligne de commande
(--cli), où l'on ne choisit pas le nom de la base. Mêmes règles que l'onglet INPUT de l'interface
(GUI/app/utils/database_name.js) : un nom non reconnu donne « Unknown ».
"""
import os
import re

# Un mot doit commencer au début du nom ou après un caractère qui n'est pas une lettre (« ALL_GNPS », « MoNA-export »),
# pour ne pas reconnaître « mona » dans « monacolin ». Les règles sont essayées dans l'ordre : la première gagne.
_START = r"(?:^|[^a-z])"
_END = r"(?:[^a-z]|$)"
RULES = [
    # Bibliothèques MSnLib (Zenodo) : un fichier par collection de molécules, « 20241003_enamdisc_pos_ms2.mgf »
    ("MSnLib", _START + r"(?:msnlib|enamdisc|enammol|mcebio|mcedrug|mcescaf|mcediv|nihnp|otavapep|targetmol)"),
    # Bibliothèques publiques de MS-DIAL : « MSMS-Public_experimentspectra-pos-VS19.msp »
    ("MS-DIAL", _START + r"(?:msms-public|msdial|ms-dial)"),
    ("Spectraverse", _START + r"spectraverse"),
    ("MoNA", _START + r"mona" + _END),
    ("GNPS", _START + r"(?:gnps|ccmslib)"),
    ("MassBank", _START + r"(?:massbank|msbnk)"),
    ("HMDB", _START + r"hmdb"),
    ("NIST", _START + r"nist"),
    ("Respect", _START + r"respect" + _END),
]
_COMPILED = [(name, re.compile(pattern)) for name, pattern in RULES]


def guess_database_name(path):
    """Base d'origine devinée d'après le nom (ou le chemin complet) du fichier, « Unknown » si rien n'est reconnu."""
    name = re.split(r"[/\\]", str(path or ""))[-1].lower()
    for database, pattern in _COMPILED:
        if pattern.search(name):
            return database
    return "Unknown"
