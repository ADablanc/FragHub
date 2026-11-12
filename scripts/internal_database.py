# -*- coding: utf-8 -*-
"""
Base interne de molécules : téléchargée depuis Zenodo quand elle n'est pas sur le disque.

La base (17 fichiers parquet, environ 1,1 Go) n'est plus dans le dépôt git. Le fichier datas/internal_database.json
dit quelle version il faut : adresse de l'archive, taille, somme SHA-256, liste des fichiers.

Règle : on ne télécharge QUE si datas/internal_databases.parquet ne contient aucun fichier .parquet. Une base déjà
présente n'est jamais remplacée ni complétée, même si elle diffère de la référence (base recalculée localement) :
on le signale seulement. Pour reprendre la base de référence, supprimer le dossier et relancer.

Le téléchargement reprend là où il s'était arrêté après une coupure. L'archive est vérifiée (taille et SHA-256)
avant d'être ouverte, puis décompressée dans un dossier provisoire renommé à la fin : le dossier de la base
n'existe donc que complet.

Utilisation directe :  python scripts/internal_database.py            (télécharge la base si elle manque)
                       python scripts/internal_database.py --check    (dit seulement dans quel état elle est)
Bibliothèque standard de Python uniquement.
"""
import hashlib
import json
import os
import shutil
import ssl
import sys
import time
import urllib.error
import urllib.request
import zipfile

REFERENCE_NAME = "internal_database.json"
DB_DIR_NAME = "internal_databases.parquet"
BLOCK = 4 * 1024 * 1024
ATTEMPTS = 5


class InternalDatabaseError(Exception):
    """La base manque et n'a pas pu être obtenue (le message dit quoi faire)."""


def _paths(base_dir):
    datas = os.path.join(base_dir, "datas")
    return datas, os.path.join(datas, REFERENCE_NAME), os.path.join(datas, DB_DIR_NAME)


def read_reference(base_dir):
    """Contenu de datas/internal_database.json."""
    _, ref_path, _ = _paths(base_dir)
    try:
        with open(ref_path, encoding="utf-8") as f:
            ref = json.load(f)
    except (OSError, ValueError) as e:
        raise InternalDatabaseError("Fichier de référence de la base illisible (%s) : %s" % (ref_path, e))
    for key in ("version", "url", "size", "sha256", "files"):
        if key not in ref:
            raise InternalDatabaseError("%s : champ « %s » absent." % (ref_path, key))
    return ref


def local_state(base_dir, ref):
    """
    État de la base sur le disque : "absente" (aucun .parquet), "reference" (mêmes fichiers et mêmes tailles que
    la référence) ou "differente" (autre chose : base recalculée localement, autre version...).
    """
    _, _, db_dir = _paths(base_dir)
    if os.path.isfile(db_dir):
        return "differente"   # un fichier parquet unique à la place du dossier : accepté par le moteur
    present = {}
    if os.path.isdir(db_dir):
        for name in os.listdir(db_dir):
            if name.endswith(".parquet"):
                present[name] = os.path.getsize(os.path.join(db_dir, name))
    if not present:
        return "absente"
    expected = {f["name"]: f["size"] for f in ref["files"]}
    return "reference" if present == expected else "differente"


def _ssl_context():
    """Certificats du système, complétés par ceux de certifi s'il est installé (Python sans magasin système)."""
    context = ssl.create_default_context()
    try:
        import certifi
        context.load_verify_locations(certifi.where())
    except Exception:
        pass
    return context


def _sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(BLOCK), b""):
            h.update(block)
    return h.hexdigest()


def _download(ref, part_path, log, progress):
    """Télécharge l'archive dans part_path, en reprenant ce qui est déjà là. Plusieurs essais."""
    total = ref["size"]
    context = _ssl_context()
    last_error = None
    for attempt in range(1, ATTEMPTS + 1):
        done = os.path.getsize(part_path) if os.path.exists(part_path) else 0
        if done > total:
            os.remove(part_path)
            done = 0
        if done == total:
            return
        request = urllib.request.Request(ref["url"], headers={"User-Agent": "FragHub"})
        if done:
            request.add_header("Range", "bytes=%d-" % done)
        try:
            with urllib.request.urlopen(request, timeout=60, context=context) as response:
                resumed = response.status == 206
                if done and not resumed:
                    done = 0   # le serveur renvoie tout depuis le début
                last_shown, t0, start = 0.0, time.time(), done
                with open(part_path, "ab" if resumed else "wb") as out:
                    for block in iter(lambda: response.read(BLOCK), b""):
                        out.write(block)
                        done += len(block)
                        now = time.time()
                        if now - last_shown >= 2 or done == total:
                            last_shown = now
                            speed = (done - start) / max(now - t0, 1e-6) / 1e6
                            log("  base interne : %5.1f %%  (%d / %d Mo, %.1f Mo/s)" % (100.0 * done / total, done // 10 ** 6, total // 10 ** 6, speed))
                            if progress:
                                progress("download", done, total)
            if done == total:
                return
            last_error = "connexion fermée après %d octets sur %d" % (done, total)
        except (urllib.error.URLError, OSError, ssl.SSLError) as e:
            last_error = str(e)
        log("  téléchargement interrompu (%s) : essai %d sur %d" % (last_error, attempt, ATTEMPTS))
        time.sleep(min(5 * attempt, 20))
    raise InternalDatabaseError(last_error or "téléchargement impossible")


def ensure_internal_database(base_dir, log=print, progress=None):
    """
    Fait en sorte que la base interne soit sur le disque. Renvoie "reference", "differente" ou "telechargee".
    progress, s'il est donné, est appelé avec (étape, octets faits, octets au total) ; les étapes sont "download",
    "verify" et "extract" (pour l'écran de démarrage de l'interface).
    Lève InternalDatabaseError (message complet, avec le lien de téléchargement à la main) si la base manque et
    ne peut pas être obtenue.
    """
    ref = read_reference(base_dir)
    datas, _, db_dir = _paths(base_dir)
    state = local_state(base_dir, ref)
    if state == "reference":
        return state
    if state == "differente":
        log("Base interne : les fichiers présents ne sont pas ceux de la version %s de référence ; ils sont utilisés tels quels." % ref["version"])
        return state

    where = "https://doi.org/%s" % ref["doi"] if ref.get("doi") else ref["url"]
    manual = ("Pour l'installer à la main : télécharger %s depuis %s et décompresser ses fichiers .parquet dans %s"
              % (os.path.basename(ref["url"].split("?")[0]), where, db_dir))
    free = shutil.disk_usage(datas).free
    if free < 2 * ref["size"] + 200 * 10 ** 6:
        raise InternalDatabaseError("Base interne absente, et pas assez de place sur le disque pour la télécharger "
                                    "(%.1f Go libres, %.1f Go nécessaires).\n%s" % (free / 1e9, (2 * ref["size"] + 200 * 10 ** 6) / 1e9, manual))
    log("Base interne absente : téléchargement de la version %s (%.2f Go) depuis Zenodo, une seule fois." % (ref["version"], ref["size"] / 1e9))
    archive = os.path.join(datas, "internal_database_%s.zip" % ref["version"])
    part = archive + ".part"
    try:
        _download(ref, part, log, progress)
    except InternalDatabaseError as e:
        raise InternalDatabaseError("Base interne absente, et son téléchargement a échoué : %s\n"
                                    "Relancer FragHub reprendra le téléchargement là où il s'est arrêté.\n%s" % (e, manual))
    except OSError as e:
        raise InternalDatabaseError("Base interne absente, et impossible d'écrire dans %s : %s\n%s" % (datas, e, manual))

    log("  vérification de l'archive (SHA-256)...")
    if progress:
        progress("verify", ref["size"], ref["size"])
    if os.path.getsize(part) != ref["size"] or _sha256(part) != ref["sha256"]:
        os.remove(part)
        raise InternalDatabaseError("L'archive téléchargée n'est pas celle attendue (taille ou somme SHA-256 différente) : "
                                    "elle a été supprimée. Relancer FragHub pour la télécharger de nouveau.\n%s" % manual)

    if progress:
        progress("extract", ref["size"], ref["size"])
    tmp_dir = db_dir + ".tmp"
    shutil.rmtree(tmp_dir, ignore_errors=True)
    os.makedirs(tmp_dir)
    expected = {f["name"]: f["size"] for f in ref["files"]}
    with zipfile.ZipFile(part) as z:
        if sorted(z.namelist()) != sorted(expected):
            raise InternalDatabaseError("L'archive ne contient pas les fichiers annoncés par la référence.")
        for name in z.namelist():
            with z.open(name) as src, open(os.path.join(tmp_dir, name), "wb") as dst:
                shutil.copyfileobj(src, dst, BLOCK)   # zipfile vérifie le CRC de chaque fichier à la lecture
            if os.path.getsize(os.path.join(tmp_dir, name)) != expected[name]:
                raise InternalDatabaseError("%s : taille différente de la référence après décompression." % name)
    try:
        if os.path.isdir(db_dir):
            os.rmdir(db_dir)   # dossier sans .parquet ; refusé s'il contient autre chose
        os.replace(tmp_dir, db_dir)
    except OSError as e:
        raise InternalDatabaseError("Base téléchargée dans %s, mais impossible de la mettre à sa place (%s).\n"
                                    "Supprimer ou vider le dossier %s puis relancer : rien ne sera retéléchargé." % (tmp_dir, e, db_dir))
    os.remove(part)
    log("Base interne installée : %d fichiers dans %s" % (len(expected), db_dir))
    return "telechargee"


if __name__ == "__main__":
    root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    try:
        if "--check" in sys.argv:
            reference = read_reference(root)
            print("Version de référence : %s ; base sur le disque : %s" % (reference["version"], local_state(root, reference)))
        else:
            print("Base interne : %s" % ensure_internal_database(root))
    except InternalDatabaseError as error:
        sys.exit(str(error))
