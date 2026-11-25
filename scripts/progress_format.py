# -*- coding: utf-8 -*-
"""
Textes des barres de progression de la ligne de commande (même présentation que l'interface,
GUI/app/utils/progress_format.js).

Unités envoyées par le moteur Rust avec chaque tâche (item_type) :
- "bytes" : lecture d'un fichier ; la progression et le total sont des octets, affichés en MB ou GB et en MB/s ou GB/s ;
- écriture des sorties : la progression compte les spectres (pourcentage et temps restant exacts) et arrive avec un
  deuxième nombre, les octets écrits, affichés en MB ou GB et en MB/s ou GB/s ;
- autre chose ("spectra", "molecules"...) : nombre d'éléments et éléments par seconde, comme avant.
Unités décimales : 1 MB = 1 000 000 octets, 1 GB = 1 000 000 000 octets (comme les tailles annoncées par Zenodo).
"""
MB = 1_000_000
GB = 1_000_000_000


def size_unit(total_bytes):
    """Unité d'affichage d'une taille : GB à partir de 1 GB, MB sinon."""
    return "GB" if total_bytes >= GB else "MB"


def format_size(n_bytes, unit=None):
    """Taille dans l'unité donnée (choisie d'après la taille si absente) : « 350.2 MB », « 1.42 GB »."""
    unit = unit or size_unit(n_bytes)
    return "{:,.2f} GB".format(n_bytes / GB) if unit == "GB" else "{:,.1f} MB".format(n_bytes / MB)


def format_rate(bytes_per_second):
    """Débit : « 120.4 MB/s », « 1.35 GB/s »."""
    return format_size(bytes_per_second) + "/s"


def progress_texts(current, total, item_type, elapsed, written_bytes=None):
    """
    (quantité, vitesse) d'une barre de progression : `current` / `total` dans l'unité de la tâche, `elapsed` en
    secondes, `written_bytes` : octets écrits (étapes d'écriture), sinon None.
    """
    if item_type == "bytes":
        unit = size_unit(total)
        counts = "%s of %s" % (format_size(current, unit)[:-3], format_size(total, unit))
        rate = format_rate(current / elapsed) if elapsed > 0 else format_rate(0)
        return counts, rate
    counts = "%s of %s %s" % (current, total, item_type)
    if written_bytes is not None:
        counts += " | %s written" % format_size(written_bytes)
        rate = format_rate(written_bytes / elapsed) if elapsed > 0 else format_rate(0)
        return counts, rate
    speed = current / elapsed if elapsed > 0 else 0
    return counts, "%.1f %s/s" % (speed, item_type)
