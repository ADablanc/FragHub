# -*- coding: utf-8 -*-
"""
Fabrique l'archive de la base interne de FragHub, à publier sur Zenodo.

La base n'est pas dans le dépôt git : FragHub la télécharge depuis Zenodo quand elle manque
(scripts/internal_database.py), d'après le fichier de référence datas/internal_database.json.

L'archive contient les fichiers part_XXXX.parquet tels quels (sans compression : ils sont déjà compressés), dans
l'ordre de leur nom et avec une date fixe : la même base donne la même archive et la même somme, à condition
d'utiliser la même version de Python (l'en-tête zip écrit par Python varie un peu d'une version à l'autre).
RIEN N'EST ENVOYÉ : le script écrit l'archive et le fichier de référence dans scratch/.

Publier une nouvelle version de la base :
  1. sur Zenodo, ouvrir le dépôt de la base, « New version » : le numéro du nouveau dépôt est réservé tout de suite
     (il est dans l'adresse du brouillon et dans son DOI) ;
  2. python scripts/build_dataset/make_archive.py --version AAAA-MM-JJ --record NUMERO
  3. déposer scratch/internal_database/internal_database_<version>.zip dans le brouillon, publier ;
  4. copier scratch/internal_database/internal_database_<version>.json sur datas/internal_database.json, commiter.

Options : --base DOSSIER (défaut datas/internal_databases.parquet), --output DOSSIER (défaut scratch/internal_database)
Python 3.7 ou plus, aucune dépendance.
"""
import argparse
import hashlib
import json
import os
import re
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BLOCK = 8 * 1024 * 1024


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(BLOCK), b""):
            h.update(block)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser(description="Fabrique l'archive de la base interne de FragHub.")
    ap.add_argument("--base", default=os.path.join(ROOT, "datas", "internal_databases.parquet"))
    ap.add_argument("--version", required=True, help="version de la base, par exemple 2026-11-12")
    ap.add_argument("--record", help="numéro du dépôt Zenodo (réservé dès le brouillon) ; sans lui, adresse et DOI restent vides")
    ap.add_argument("--output", default=os.path.join(ROOT, "scratch", "internal_database"))
    a = ap.parse_args()
    if not re.match(r"^[0-9A-Za-z][0-9A-Za-z._-]*$", a.version):
        sys.exit("Version refusée (lettres, chiffres, point, tiret, souligné) : %s" % a.version)
    if a.record and not a.record.isdigit():
        sys.exit("--record attend le numéro du dépôt Zenodo (des chiffres) : %s" % a.record)
    names = sorted(f for f in os.listdir(a.base) if re.match(r"^part_\d{4}\.parquet$", f))
    if not names:
        sys.exit("Aucun fichier part_XXXX.parquet dans %s" % a.base)
    other = sorted(f for f in os.listdir(a.base) if f not in names)
    if other:
        print("Laissés hors de l'archive : %s" % ", ".join(other))

    os.makedirs(a.output, exist_ok=True)
    name = "internal_database_%s.zip" % a.version
    dst = os.path.join(a.output, name)
    files = []
    with zipfile.ZipFile(dst + ".tmp", "w", zipfile.ZIP_STORED, allowZip64=True) as z:
        for f in names:
            src = os.path.join(a.base, f)
            info = zipfile.ZipInfo(f, date_time=(1980, 1, 1, 0, 0, 0))   # date fixe : archive reproductible
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o644 << 16
            with open(src, "rb") as i, z.open(info, "w", force_zip64=True) as o:
                for block in iter(lambda: i.read(BLOCK), b""):
                    o.write(block)
            files.append({"name": f, "size": os.path.getsize(src), "sha256": sha256(src)})
            print("  %s ajouté (%d octets)" % (f, files[-1]["size"]), flush=True)

    # relecture : chaque fichier de l'archive doit être identique à sa source
    with zipfile.ZipFile(dst + ".tmp") as z:
        if z.namelist() != names:
            raise RuntimeError("liste des fichiers de l'archive inattendue")
        for entry in files:
            h = hashlib.sha256()
            with z.open(entry["name"]) as i:
                for block in iter(lambda: i.read(BLOCK), b""):
                    h.update(block)
            if h.hexdigest() != entry["sha256"]:
                raise RuntimeError("%s : contenu de l'archive différent de la source" % entry["name"])
    os.replace(dst + ".tmp", dst)

    url = "https://zenodo.org/records/%s/files/%s?download=1" % (a.record, name) if a.record else ""
    doi = "10.5281/zenodo.%s" % a.record if a.record else ""
    reference = {"version": a.version, "doi": doi, "url": url, "size": os.path.getsize(dst), "sha256": sha256(dst), "files": files}
    ref_path = os.path.join(a.output, "internal_database_%s.json" % a.version)
    with open(ref_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(reference, f, indent=2)
        f.write("\n")

    print("\nArchive   : %s (%d octets, %d fichiers, relue et identique aux sources)" % (dst, reference["size"], len(files)))
    print("SHA-256   : %s" % reference["sha256"])
    print("Référence : %s" % ref_path)
    if a.record:
        print("Après publication sur Zenodo, copier la référence sur datas/internal_database.json et commiter.")
    else:
        print("Sans --record : l'adresse et le DOI de la référence sont vides, à compléter avant de la copier dans datas/.")


if __name__ == "__main__":
    main()
