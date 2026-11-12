# -*- coding: utf-8 -*-
"""
Ajoute à la base interne les molécules des spectres qui n'y sont pas encore.

Lit les sorties CSV d'un run complet de FragHub (spectres GARDÉS, expérimentaux et in silico), garde les molécules
dont l'InChIKey est absente de la base, et écrit pour elles un NOUVEAU fichier de la base (part_XXXX.parquet) :
  - INCHIKEY, INCHI, SMILES, FORMULA, EXACTMASS, AVERAGEMASS : ceux que FragHub vient de calculer (même traitement
    RDKit que la base re-normalisée) ; si une clé a plusieurs écritures dans le run, la plus fréquente ;
  - NAME : le nom le plus fréquent parmi les spectres de la molécule (à égalité, le premier rencontré) ;
  - NPCLASS_* : classes de COCONUT si l'InChIKey complète y est, sinon modèle NP-Classifier (voir npclassifier_db.py,
    RDKit 2020.03.6 obligatoire) ; SMILES illisible -> « NOT FOUND » ;
  - CLASSYFIRE_* : pas de modèle local. Reprises d'une molécule de la base qui ne diffère que par la stéréochimie
    (même premier bloc d'InChIKey = même enchaînement d'atomes, même état de protonation), et seulement si toutes
    les voisines classées de la base donnent les mêmes classes ; sinon vides.
Les fichiers existants de la base ne sont pas modifiés. Rien n'est écrit dans datas/ : le fichier est écrit dans le
dossier de sortie, à copier ensuite.

Usage (depuis la racine du projet) :
  %USERPROFILE%\\rdkit_npc\\python.exe scripts\\build_dataset\\add_new_molecules.py --run C:\\...\\OUT_PC_FULL_3 --coconut C:\\...\\coconut_csv-11-2026.csv
Options : --input BASE (défaut datas/internal_databases.parquet), --output DOSSIER (défaut scratch/new_molecules),
          --dry-run (compte et liste les molécules nouvelles, sans classes ni parquet)
Sorties : <sortie>/part_XXXX.parquet, _nouvelles_molecules.tsv (une ligne par molécule), _journal.txt
"""
import argparse
import csv
import glob
import os
import sys
import time
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import npclassifier_db as npc   # règle aussi le chemin des DLL sous Windows

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq

ROOT = npc.ROOT
FIELDS = ("INCHI", "SMILES", "FORMULA", "EXACTMASS", "AVERAGEMASS")
EMPTY = ("", "NOT FOUND", "nan", "None")
CLASSYFIRE = ("CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS")


def classyfire_from_neighbours(base_dir, names, keys):
    """
    Classes ClassyFire reprises des voisines de la base : même premier bloc (14 lettres) et même protonation
    (dernière lettre), stéréochimie différente. Renvoie clé nouvelle -> (classes, clé de la base utilisée), et le
    nombre de molécules dont les voisines se contredisent (laissées vides).
    """
    wanted = {k[:14] for k in keys}
    found = {}   # (premier bloc, protonation) -> {classes: première clé de la base qui les porte}
    for f in names:
        t = pq.read_table(os.path.join(base_dir, f), columns=["INCHIKEY"] + list(CLASSYFIRE))
        cols = [t.column(i).to_pylist() for i in range(4)]
        for k, a, b, c in zip(*cols):
            if not k or k[:14] not in wanted or a is None or a in EMPTY:
                continue
            found.setdefault((k[:14], k[25:]), {}).setdefault((a, b, c), k)
    inherited, conflicts = {}, 0
    for k in keys:
        options = found.get((k[:14], k[25:]))
        if not options:
            continue
        if len(options) > 1:
            conflicts += 1
            continue
        (cf, source), = options.items()
        inherited[k] = (cf, source)
    return inherited, conflicts


def most_common_first(counter):
    """Valeur la plus fréquente ; à égalité, la première rencontrée (Counter garde l'ordre d'insertion)."""
    best, best_n = None, 0
    for value, n in counter.items():
        if n > best_n:
            best, best_n = value, n
    return best


def read_run(run_dir, base_keys):
    """Molécules des spectres gardés absentes de la base : clé -> compteurs. Renvoie aussi des comptes."""
    csv.field_size_limit(2 ** 31 - 1)
    new, stats = {}, Counter()
    files = sorted(glob.glob(os.path.join(run_dir, "CSV", "*", "*.csv")))
    if not files:
        sys.exit("Aucun fichier CSV dans %s" % os.path.join(run_dir, "CSV"))
    for path in files:
        label = os.path.basename(path)[:-4]
        with open(path, newline="", encoding="utf-8") as h:
            r = csv.reader(h, delimiter=";")
            head = next(r, None)
            if not head:
                continue
            pos = {c: i for i, c in enumerate(head)}
            missing = [c for c in ("INCHIKEY", "NAME", "FILENAME") + FIELDS if c not in pos]
            if missing:
                sys.exit("%s : colonnes absentes : %s" % (path, ", ".join(missing)))
            for row in r:
                stats["spectres"] += 1
                key = row[pos["INCHIKEY"]].strip()
                if key in EMPTY:
                    stats["spectres sans InChIKey"] += 1
                    continue
                if key in base_keys:
                    stats["spectres dont la molécule est dans la base"] += 1
                    continue
                stats["spectres dont la molécule est nouvelle"] += 1
                m = new.get(key)
                if m is None:
                    m = new[key] = {"n": 0, "NAME": Counter(), "files": Counter(), "libs": Counter()}
                    for f in FIELDS:
                        m[f] = Counter()
                m["n"] += 1
                m["files"][row[pos["FILENAME"]]] += 1
                m["libs"][label] += 1
                name = row[pos["NAME"]].strip()
                if name not in EMPTY:
                    m["NAME"][name] += 1
                for f in FIELDS:
                    v = row[pos[f]].strip()
                    if v not in EMPTY:
                        m[f][v] += 1
        print("  %-28s lu (%d molécules nouvelles jusqu'ici)" % (label, len(new)), flush=True)
    return new, stats


def to_float(text):
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


def column(values, field):
    """Colonne du type de la base (dictionnaire, texte ou nombre)."""
    if pa.types.is_dictionary(field.type):
        return npc.dictionary_column(values, field)
    if pa.types.is_floating(field.type):
        return pa.array([to_float(v) for v in values], type=field.type)
    return pa.array(values, type=field.type)


def main():
    ap = argparse.ArgumentParser(description="Ajoute à la base interne les molécules nouvelles d'un run.")
    ap.add_argument("--run", required=True, help="dossier de sortie d'un run complet de FragHub")
    ap.add_argument("--coconut", help="coconut_csv-11-2026.csv (obligatoire sauf --dry-run)")
    ap.add_argument("--input", default=os.path.join(ROOT, "datas", "internal_databases.parquet"))
    ap.add_argument("--output", default=os.path.join(ROOT, "scratch", "new_molecules"))
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    if not a.dry_run:
        if not a.coconut:
            sys.exit("--coconut est obligatoire (sauf avec --dry-run).")
        version = npc.check_rdkit()
    os.makedirs(a.output, exist_ok=True)
    journal = open(os.path.join(a.output, "_journal.txt"), "a", encoding="utf-8")

    def log(message):
        print(message)
        journal.write(time.strftime("%Y-%m-%d %H:%M:%S ") + message + "\n")
        journal.flush()

    t0 = time.time()
    names = sorted(f for f in os.listdir(a.input) if f.endswith(".parquet"))
    new_name = "part_%04d.parquet" % (max(int(f[5:9]) for f in names) + 1)
    base_keys = set()
    for f in names:
        base_keys.update(k for k in pq.read_table(os.path.join(a.input, f), columns=["INCHIKEY"]).column(0).to_pylist() if k)
    log("Base %s : %d fichiers, %d InChIKey ; run %s%s" % (a.input, len(names), len(base_keys), a.run, " ; CONSTAT SEUL" if a.dry_run else ""))

    new, stats = read_run(a.run, base_keys)
    for k in ("spectres", "spectres sans InChIKey", "spectres dont la molécule est dans la base", "spectres dont la molécule est nouvelle"):
        log("  %-45s %d" % (k, stats[k]))
    keys = sorted(new)
    several = {f: sum(1 for k in keys if len(new[k][f]) > 1) for f in ("NAME",) + FIELDS}
    no_structure = sum(1 for k in keys if not new[k]["SMILES"])
    log("Molécules nouvelles : %d (sans SMILES : %d) ; plusieurs écritures dans le run : %s"
        % (len(keys), no_structure, ", ".join("%s %d" % kv for kv in several.items())))
    libs = Counter()
    for k in keys:
        libs.update(new[k]["libs"].keys())
    log("Par bibliothèque (une molécule peut être dans plusieurs) : %s" % ", ".join("%s %d" % kv for kv in libs.most_common()))
    sources = Counter()
    for k in keys:
        sources.update(new[k]["files"].keys())
    log("Par fichier d'entrée : %s" % ", ".join("%s %d" % kv for kv in sources.most_common(15)))

    rows = {c: [] for c in ("INCHIKEY", "NAME") + FIELDS}
    for k in keys:
        rows["INCHIKEY"].append(k)
        rows["NAME"].append(most_common_first(new[k]["NAME"]) or "")
        for f in FIELDS:
            rows[f].append(most_common_first(new[k][f]))

    inherited, conflicts = classyfire_from_neighbours(a.input, names, keys)
    log("ClassyFire repris d'une voisine de la base (stéréochimie seule différente) : %d molécules ; voisines en désaccord, laissées vides : %d"
        % (len(inherited), conflicts))

    classes = [(None, None, None)] * len(keys)
    source = [""] * len(keys)
    if not a.dry_run:
        clf = npc.NPClassifier()
        coco, _ = npc.read_coconut(a.coconut, clf.index)
        todo = []
        for i, k in enumerate(keys):
            if k in coco and rows["SMILES"][i]:
                classes[i], source[i] = coco[k], "COCONUT"
            else:
                todo.append(i)
        log("RDKit %s ; COCONUT : %d molécules ; modèle : %d" % (version, len(keys) - len(todo), len(todo)))
        unreadable = 0
        for b in range(0, len(todo), 2000):
            part = todo[b:b + 2000]
            sparse = npc.fingerprints_sparse([rows["SMILES"][i] or "" for i in part])
            ok = [(i, fp) for i, fp in zip(part, sparse) if fp is not None]
            for i in part:
                classes[i], source[i] = (npc.NOT_FOUND,) * 3, "SMILES illisible"
            unreadable += len(part) - len(ok)
            for c in range(0, len(ok), npc.BATCH):
                chunk = ok[c:c + npc.BATCH]
                for (i, _), res in zip(chunk, clf.classify_batch(npc.dense_batch([fp for _, fp in chunk]))):
                    classes[i], source[i] = res, "modèle"
            print("\r  %d/%d molécules classées par le modèle" % (min(b + 2000, len(todo)), len(todo)), end="", flush=True)
        print()
        log("SMILES illisibles par RDKit 2020 (classes « NOT FOUND ») : %d" % unreadable)
        log("Voies : %s" % ", ".join("%s %d" % kv for kv in Counter(c[0] for c in classes).most_common(10)))

    listing = os.path.join(a.output, "_nouvelles_molecules.tsv")
    with open(listing, "w", newline="", encoding="utf-8") as h:
        w = csv.writer(h, delimiter="\t", lineterminator="\n")
        w.writerow(["INCHIKEY", "spectres", "NAME", "autres_noms", "FORMULA", "SMILES", "bibliotheques", "fichiers_entree",
                    "source_classes", "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS", "classyfire_repris_de"] + list(CLASSYFIRE))
        for i, k in enumerate(keys):
            m = new[k]
            w.writerow([k, m["n"], rows["NAME"][i], len(m["NAME"]) - 1 if m["NAME"] else 0, rows["FORMULA"][i] or "", rows["SMILES"][i] or "",
                        "|".join(sorted(m["libs"])), "|".join(sorted(m["files"])), source[i]] + [c or "" for c in classes[i]]
                       + ([inherited[k][1]] + [c or "" for c in inherited[k][0]] if k in inherited else ["", "", "", ""]))
    log("Liste écrite : %s" % listing)
    if a.dry_run:
        log("Constat seul : aucun parquet écrit. (%.0f s)" % (time.time() - t0))
        return

    schema = pq.read_schema(os.path.join(a.input, names[0]))
    data = dict(rows)
    for c, col in enumerate(npc.COLUMNS):
        data[col] = [cl[c] for cl in classes]
    for c, col in enumerate(CLASSYFIRE):
        data[col] = [inherited[k][0][c] if k in inherited else None for k in keys]
    arrays = []
    for field in schema:
        values = data.get(field.name)
        if values is None:
            values = [None] * len(keys)   # toute autre colonne : vide
        arrays.append(column(values, field))
    table = pa.Table.from_arrays(arrays, schema=pa.schema([pa.field(f.name, arr.type) for f, arr in zip(schema, arrays)], metadata=schema.metadata))
    dst = os.path.join(a.output, new_name)
    pq.write_table(table, dst + ".tmp", compression="zstd", row_group_size=max(table.num_rows, 1))
    with open(dst + ".tmp", "rb") as handle:
        written = pq.ParquetFile(handle).metadata.num_rows
    if written != len(keys):
        raise RuntimeError("%d lignes écrites au lieu de %d" % (written, len(keys)))
    os.replace(dst + ".tmp", dst)
    log("%s : %d molécules écrites (%.0f s). À copier dans la base après vérification." % (dst, written, time.time() - t0))
    journal.close()


if __name__ == "__main__":
    main()
