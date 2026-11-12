# -*- coding: utf-8 -*-
"""
Une seule ligne par InChIKey dans la base interne (datas/internal_databases.parquet).

Pourquoi : la re-normalisation neutralise les molécules ; les formes chargées de PubChem (« Tafamidis(1-) »,
« (2,5-Dimethylphenyl)azanium », « 3-Amino-2-methylbenzoate ») tombent alors sur l'InChIKey de la molécule neutre.
FragHub garde la dernière ligne lue pour une clé : le nom et les classes ClassyFire d'une molécule dépendaient donc
de l'ordre des fichiers (« Azithromycin » -> « CID 55185 », classes ClassyFire perdues).

Dans chaque groupe de lignes de même InChIKey, la ligne gardée est, dans l'ordre :
  1. celle qui était déjà neutre AVANT la re-normalisation (InChI de l'ancienne base sans couche de charge /q ni /p) ;
  2. sinon celle qui a des classes ClassyFire ;
  3. sinon celle dont le nom n'est pas un identifiant (« CID 123 », « SCHEMBL123 »…) ;
  4. sinon la première dans l'ordre de lecture.
Si la ligne gardée n'a pas de classes ClassyFire et qu'une autre ligne du groupe en a, elle les reprend (même
molécule). Les groupes dont les formules diffèrent ne sont PAS touchés : ils sont listés pour être regardés.

Lignes « classes seules » : la base contient aussi des lignes qui n'ont qu'une InChIKey et des classes (ni structure,
ni nom, ni formule : tout part_0015 et une partie de part_0014). Quand la même InChIKey a aussi une ligne complète,
c'est la ligne complète qui est gardée (choisie parmi les lignes complètes avec les critères ci-dessus) ; elle
reprend les classes ClassyFire de la ligne « classes seules » si elle n'en a pas, et celle-ci est retirée. Une
formule vide en face d'une formule remplie n'est donc pas une « formule différente ».

L'ancienne base (avant re-normalisation) sert au critère 1. Elle est lue dans un dossier (--old-dir) ou dans
l'historique git (--old-commit) ; sans l'un ni l'autre, le critère 1 n'est pas appliqué. Depuis que la base est
sortie du dépôt (historique réécrit, base publiée sur Zenodo), elle n'est plus dans l'historique git. La sortie est écrite dans un NOUVEAU dossier (nom temporaire puis renommage) ; la base d'origine n'est pas
modifiée. Les fichiers sans ligne retirée ni classe reprise sont recopiés octet pour octet.

Usage (depuis la racine du projet, environnement du classifieur pour pyarrow) :
  %USERPROFILE%\\rdkit_npc\\python.exe scripts\\build_dataset\\dedupe_db.py
Options : --input, --output (défaut scratch/dedupe_db/internal_databases.parquet), --old-dir DOSSIER,
          --old-commit COMMIT, --dry-run (constat seulement, aucun parquet écrit)
Journal : <sortie>/_dedupe_journal.txt ; lignes retirées : _dedupe_lignes_retirees.tsv ;
classes reprises : _dedupe_classes_reprises.tsv ; groupes laissés : _dedupe_groupes_laisses.tsv
"""
import argparse
import csv
import io
import os
import re
import shutil
import subprocess
import sys
import time
from collections import defaultdict

if sys.platform == 'win32':
    # Environnement conda lancé sans activation : ses DLL sont dans Library\bin (voir npclassifier_db.py)
    _dirs = [d for d in (os.path.join(sys.prefix, 'Library', 'bin'), sys.prefix) if os.path.isdir(d)]
    os.environ['PATH'] = os.pathsep.join(_dirs + [os.environ.get('PATH', '')])
    if hasattr(os, 'add_dll_directory'):
        for _d in _dirs:
            os.add_dll_directory(_d)

import numpy as np
import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.parquet as pq

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CLASSYFIRE = ("CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS")
NOT_FOUND = "NOT FOUND"
IDENTIFIER = re.compile(r"^(CID \d+|SID \d+|SCHEMBL\d+|CHEMBL\d+|ZINC\d+|MCULE-\d+|AKOS\d+)$", re.I)
CHARGE_LAYER = re.compile(r"/[qp][^/]")


def is_charged_inchi(inchi):
    """InChI avec une couche de charge (/q) ou de protons ajoutés / retirés (/p). None si l'InChI manque."""
    if not inchi:
        return None
    return bool(CHARGE_LAYER.search(inchi))


def has_classyfire(values):
    return bool(values[0]) and values[0] != NOT_FOUND


def read_old(name, old_dir, commit):
    """NAME et INCHI de l'ancienne base (avant re-normalisation) pour ce fichier, ou None s'il est introuvable."""
    if not old_dir and not commit:
        return None
    try:
        if old_dir:
            source = os.path.join(old_dir, name)
        else:
            data = subprocess.run(["git", "show", "%s:datas/internal_databases.parquet/%s" % (commit, name)],
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, cwd=ROOT).stdout
            source = io.BytesIO(data)
        t = pq.read_table(source, columns=["NAME", "INCHI"])
        return t.column("NAME").to_pylist(), t.column("INCHI").to_pylist()
    except Exception as e:  # fichier absent du commit, git introuvable…
        print("  ancienne base illisible pour %s : %s" % (name, str(e)[:200]))
        return None


def align(new_names, old_names):
    """Pour chaque ligne de la nouvelle base, la ligne de l'ancienne. La re-normalisation n'a fait que RETIRER des
    lignes (molécules trop longues) et n'a pas touché aux noms : on avance dans l'ancienne base en sautant les lignes
    dont le nom ne correspond pas. Renvoie None si l'alignement n'est pas cohérent."""
    out, j, n_old = [], 0, len(old_names)
    for name in new_names:
        while j < n_old and old_names[j] != name:
            j += 1
        if j >= n_old:
            return None
        out.append(j)
        j += 1
    return out


def dictionary_column(values, field):
    """Colonne du même type que la colonne d'origine (index élargi en uint32 s'il est trop petit)."""
    if not pa.types.is_dictionary(field.type):
        return pa.array(values, type=field.type)
    arr = np.array([v if v is not None else "" for v in values], dtype=object)
    mask = np.array([v is None for v in values], dtype=bool)
    uniq, inverse = np.unique(arr, return_inverse=True)
    index_type = field.type.index_type
    limits = {pa.int8(): 127, pa.int16(): 32767, pa.int32(): 2 ** 31 - 1, pa.uint8(): 255, pa.uint16(): 65535}
    if len(uniq) > limits.get(index_type, 2 ** 32 - 1):
        index_type = pa.uint32()
    indices = pa.array(inverse.astype(np.int64), type=index_type, mask=mask if mask.any() else None)
    return pa.DictionaryArray.from_arrays(indices, pa.array(list(uniq), type=field.type.value_type))


def main():
    ap = argparse.ArgumentParser(description="Une seule ligne par InChIKey dans la base interne.")
    ap.add_argument("--input", default=os.path.join(ROOT, "datas", "internal_databases.parquet"))
    ap.add_argument("--output", default=os.path.join(ROOT, "scratch", "dedupe_db", "internal_databases.parquet"))
    ap.add_argument("--old-commit", default=None, help="commit de la base AVANT la re-normalisation (si elle est dans git)")
    ap.add_argument("--old-dir", default=None, help="ancienne base dans un dossier (à la place de git)")
    ap.add_argument("--dry-run", action="store_true", help="constat seulement : aucun parquet écrit")
    a = ap.parse_args()
    if os.path.abspath(a.input) == os.path.abspath(a.output):
        sys.exit("La sortie doit être un nouveau dossier : la base d'origine n'est jamais modifiée.")
    os.makedirs(a.output, exist_ok=True)
    journal = open(os.path.join(a.output, "_dedupe_journal.txt"), "a", encoding="utf-8")

    def log(message):
        print(message)
        journal.write(time.strftime("%Y-%m-%d %H:%M:%S ") + message + "\n")
        journal.flush()

    t0 = time.time()
    names = sorted(f for f in os.listdir(a.input) if f.endswith(".parquet"))
    log("Entrée %s ; sortie %s%s" % (a.input, a.output, " ; CONSTAT SEUL (--dry-run)" if a.dry_run else ""))

    # 1. Clés présentes sur plusieurs lignes (toute la base)
    keys = {f: pq.read_table(os.path.join(a.input, f), columns=["INCHIKEY"]).column("INCHIKEY").combine_chunks() for f in names}
    keys = {f: (k.dictionary_decode() if pa.types.is_dictionary(k.type) else k) for f, k in keys.items()}
    counts = pc.value_counts(pa.chunked_array([k.cast(pa.large_string()) for k in keys.values()]))
    dup_keys = set(k for k, n in zip(counts.field("values").to_pylist(), counts.field("counts").to_pylist()) if k and n > 1)
    total = sum(len(k) for k in keys.values())
    log("%d lignes ; %d InChIKey présentes sur plusieurs lignes" % (total, len(dup_keys)))

    # 2. Détail des lignes de ces groupes, dans l'ordre de lecture (fichier, ligne)
    groups = defaultdict(list)   # clé -> [ligne]
    no_old = []
    for f in names:
        rows = [i for i, k in enumerate(keys[f].to_pylist()) if k in dup_keys]
        if not rows:
            continue
        t = pq.read_table(os.path.join(a.input, f), columns=["INCHIKEY", "NAME", "FORMULA"] + list(CLASSYFIRE)).to_pydict()
        old = read_old(f, a.old_dir, a.old_commit)
        mapping = align(t["NAME"], old[0]) if old else None
        if old and mapping is None:
            print("  %s : alignement avec l'ancienne base impossible" % f)
        if mapping is None:
            no_old.append(f)
        for i in rows:
            charged = is_charged_inchi(old[1][mapping[i]]) if mapping is not None else None
            groups[t["INCHIKEY"][i]].append({
                "file": f, "row": i, "name": t["NAME"][i] or "", "formula": t["FORMULA"][i] or "",
                "cf": tuple(t[c][i] for c in CLASSYFIRE), "charged": charged})
    if no_old:
        log("ATTENTION : ancienne base indisponible pour %s : le critère 1 (ligne déjà neutre) n'y est pas appliqué" % ", ".join(no_old))

    # 3. Choix dans chaque groupe
    drop = defaultdict(set)        # fichier -> lignes à retirer
    inherit = defaultdict(dict)    # fichier -> ligne -> classes ClassyFire reprises
    stats = defaultdict(int)
    out = a.output
    f_removed = open(os.path.join(out, "_dedupe_lignes_retirees.tsv"), "w", newline="", encoding="utf-8")
    f_inherit = open(os.path.join(out, "_dedupe_classes_reprises.tsv"), "w", newline="", encoding="utf-8")
    f_left = open(os.path.join(out, "_dedupe_groupes_laisses.tsv"), "w", newline="", encoding="utf-8")
    w_removed = csv.writer(f_removed, delimiter="\t", lineterminator="\n")
    w_inherit = csv.writer(f_inherit, delimiter="\t", lineterminator="\n")
    w_left = csv.writer(f_left, delimiter="\t", lineterminator="\n")
    w_removed.writerow(["INCHIKEY", "fichier", "ligne", "nom_retire", "nom_garde", "fichier_garde", "critere"])
    w_inherit.writerow(["INCHIKEY", "nom_garde"] + list(CLASSYFIRE) + ["nom_source", "autres_classes_differentes"])
    w_left.writerow(["INCHIKEY", "fichier", "ligne", "nom", "formule"])
    for key in sorted(groups):
        g = groups[key]
        # Lignes « classes seules » (ni formule ni nom) : mises à part quand le groupe a aussi des lignes complètes
        complete = [r for r in g if r["formula"] or r["name"]]
        classes_only = [r for r in g if not (r["formula"] or r["name"])] if complete else []
        candidates = complete or g
        if len(set(r["formula"] for r in candidates)) > 1:
            stats["groupes laissés (formules différentes)"] += 1
            for r in g:
                w_left.writerow([key, r["file"], r["row"], r["name"], r["formula"]])
            continue
        # Clé de tri : la meilleure ligne en premier ; l'ordre de lecture départage (tri stable)
        rank = lambda r: (0 if r["charged"] is False else 1, 0 if has_classyfire(r["cf"]) else 1, 1 if IDENTIFIER.match(r["name"].strip()) else 0)
        ordered = sorted(candidates, key=rank) + classes_only
        best = ordered[0]
        ranks = [rank(r) for r in ordered[:len(candidates)]]
        if len(candidates) == 1:
            reason = "ligne complète (l'autre n'a que des classes)"
        elif ranks[0][0] < ranks[1][0]:
            reason = "déjà neutre avant la re-normalisation"
        elif ranks[0][1] < ranks[1][1]:
            reason = "a des classes ClassyFire"
        elif ranks[0][2] < ranks[1][2]:
            reason = "nom non identifiant"
        else:
            reason = "première dans l'ordre de lecture"
        if classes_only:
            stats["groupes avec une ligne « classes seules » retirée"] += 1
        stats["groupes traités"] += 1
        stats["critère : " + reason] += 1
        if best is not g[-1]:
            stats["groupes où la ligne gardée change (avant : la dernière lue)"] += 1
        for r in g:
            if r is not best:
                drop[r["file"]].add(r["row"])
                stats["lignes retirées"] += 1
                w_removed.writerow([key, r["file"], r["row"], r["name"], best["name"], best["file"], reason])
        if not has_classyfire(best["cf"]):
            donors = [r for r in ordered[1:] if has_classyfire(r["cf"])]
            if donors:
                inherit[best["file"]][best["row"]] = donors[0]["cf"]
                differing = len(set(r["cf"] for r in donors)) > 1
                stats["classes ClassyFire reprises d'une autre ligne"] += 1
                stats["  dont plusieurs lignes avec des classes différentes"] += differing
                w_inherit.writerow([key, best["name"]] + list(donors[0]["cf"]) + [donors[0]["name"], "oui" if differing else "non"])
    for h in (f_removed, f_inherit, f_left):
        h.close()
    for k in sorted(stats):
        log("  %-62s %d" % (k, stats[k]))

    # 4. Écriture
    if a.dry_run:
        log("Constat seul : aucun parquet écrit. (%.0f s)" % (time.time() - t0))
        return
    written = 0
    for f in names:
        src, dst = os.path.join(a.input, f), os.path.join(out, f)
        if not drop[f] and not inherit[f]:
            shutil.copyfile(src, dst + ".tmp")
            os.replace(dst + ".tmp", dst)
            written += len(keys[f])
            log("%s : recopié tel quel (%d lignes)" % (f, len(keys[f])))
            continue
        table = pq.read_table(src)
        n = table.num_rows
        if inherit[f]:
            for c, column in enumerate(CLASSYFIRE):
                pos = table.schema.get_field_index(column)
                values = table.column(pos).to_pylist()
                for row, cf in inherit[f].items():
                    values[row] = cf[c]
                table = table.set_column(pos, table.schema.field(pos).name, dictionary_column(values, table.schema.field(pos)))
        if drop[f]:
            keep = np.ones(n, dtype=bool)
            keep[list(drop[f])] = False
            table = table.filter(pa.array(keep))
        pq.write_table(table, dst + ".tmp", compression="zstd", row_group_size=max(table.num_rows, 1))
        with open(dst + ".tmp", "rb") as handle:   # refermé avant le renommage (Windows)
            check = pq.ParquetFile(handle).metadata.num_rows
        if check != n - len(drop[f]):
            raise RuntimeError("%s : %d lignes écrites au lieu de %d" % (f, check, n - len(drop[f])))
        os.replace(dst + ".tmp", dst)
        written += check
        log("%s : %d lignes -> %d (%d retirées, %d classes ClassyFire reprises)" % (f, n, check, len(drop[f]), len(inherit[f])))
    log("TOTAL : %d lignes -> %d (%d retirées) ; %.0f s" % (total, written, total - written, time.time() - t0))
    journal.close()


if __name__ == "__main__":
    main()
