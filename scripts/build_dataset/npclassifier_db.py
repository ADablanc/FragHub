# -*- coding: utf-8 -*-
"""
Recalcule les classes NPClassifier (NPCLASS_PATHWAY, NPCLASS_SUPERCLASS, NPCLASS_CLASS) de la base interne
(datas/internal_databases.parquet).

Pourquoi : les modèles NP-Classifier V1 (datas/npclassifier_models) ont été entraînés avec les empreintes Morgan de
RDKit 2020. Le hachage de ces empreintes a changé dans RDKit 2022.03 : calculées avec un RDKit récent, elles n'ont
plus de sens pour le réseau et les classes sont fausses. Ce script doit donc tourner dans un environnement À PART,
avec RDKit 2020.03.6 (il refuse de tourner avec RDKit 2022.03 ou plus récent) :

  mamba create -y -p %USERPROFILE%\\rdkit_npc -c conda-forge python=3.7 rdkit=2020.03.6 numpy pyarrow h5py

Pour chaque ligne de la base :
  - SMILES lisible et InChIKey complète présente dans COCONUT avec des classes -> classes de COCONUT ;
  - sinon, SMILES lisible -> modèle NP-Classifier (réécriture du code officiel, voir plus bas) ;
  - sinon (pas de SMILES, ou SMILES illisible) -> valeurs actuelles gardées.
Les colonnes ClassyFire et toutes les autres colonnes sont recopiées sans changement. Les valeurs multiples d'une
cellule sont triées par ordre alphabétique et jointes par « | » ; rien -> « NOT FOUND ».

La sortie est écrite dans un NOUVEAU dossier, un fichier à la fois (nom temporaire puis renommage) : la base
d'origine n'est pas modifiée. Sauvegarde au fur et à mesure : toutes les 20 000 molécules, les classes calculées
sont ajoutées à un fichier de reprise (<fichier>.reprise). Après un arrêt (Ctrl+C, coupure, erreur), relancer la
même commande saute les fichiers déjà écrits et reprend le fichier en cours là où il s'était arrêté.
Un fichier sans aucun SMILES est recopié octet pour octet.

Usage (depuis la racine du projet, Windows) :
  %USERPROFILE%\\rdkit_npc\\python.exe scripts\\build_dataset\\npclassifier_db.py --coconut C:\\...\\coconut_csv-11-2026.csv
Options :
  --input DOSSIER    base à lire (défaut : datas/internal_databases.parquet)
  --output DOSSIER   base à écrire (défaut : scratch/npclassifier_db/internal_databases.parquet)
  --only NOM [NOM…]  ne traiter que ces fichiers (ex. part_0003.parquet)
  --procs N          processus pour le calcul des empreintes (défaut : nombre de cœurs)
  --max N            ESSAI : ne traite que les N premières lignes de chaque fichier, n'écrit AUCUN parquet, et mesure
                     l'accord du modèle avec COCONUT sur les lignes qui y sont (journal et échantillon écrits)
Journal : <sortie>/_npclassifier_journal.txt ; échantillon des changements : <sortie>/_npclassifier_changements.tsv
"""
import argparse
import csv
import itertools
import json
import multiprocessing
import os
import shutil
import sys
import time

# Windows : l'environnement conda est lancé sans être « activé » (python.exe appelé directement) ; ses DLL (numpy,
# RDKit, pyarrow) sont dans <environnement>\Library\bin, qu'il faut ajouter à la recherche avant tout import.
# Les processus du calcul des empreintes relisent ce fichier : le réglage vaut aussi pour eux.
if sys.platform == 'win32':
    _dll_dirs = [os.path.join(sys.prefix, 'Library', 'bin'), os.path.join(sys.prefix, 'Library', 'mingw-w64', 'bin'),
                 os.path.join(sys.prefix, 'Library', 'usr', 'bin'), os.path.join(sys.prefix, 'Scripts'), sys.prefix]
    _dll_dirs = [d for d in _dll_dirs if os.path.isdir(d)]
    os.environ['PATH'] = os.pathsep.join(_dll_dirs + [os.environ.get('PATH', '')])
    if hasattr(os, 'add_dll_directory'):
        for _d in _dll_dirs:
            os.add_dll_directory(_d)

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MODELS = os.path.join(ROOT, "datas", "npclassifier_models")
COLUMNS = ("NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS")
NOT_FOUND = "NOT FOUND"
BLOCK = 20000        # lignes classées par le modèle entre deux affichages
CHUNK = 500          # SMILES par tâche d'empreinte
BATCH = 256          # molécules par passage dans les réseaux (comme la version validée)
SAMPLE_PER_FILE = 300  # changements notés par fichier dans l'échantillon TSV


# ----------------------------------------------------------------------------------------------------------------
# NP-Classifier, réécrit d'après le code officiel (github.com/mwang87/NP-Classifier : Classifier/fingerprint_handler.py,
# Classifier/prediction_voting.py, app.py). Validé contre COCONUT (2 000 molécules, RDKit 2020.03.6) : voie identique
# 94 %, trois niveaux identiques 86 %. NE PAS MODIFIER le calcul sans refaire cette validation.
# ----------------------------------------------------------------------------------------------------------------

def fingerprint(smiles, radi=2):
    """Copie de fingerprint_handler.calculate_fingerprint. None si le SMILES est illisible."""
    from rdkit import Chem
    from rdkit.Chem import rdMolDescriptors
    binary = np.zeros((2048 * radi), int)
    formula = np.zeros((2048), int)
    mol = Chem.MolFromSmiles(smiles)
    if mol is None:
        return None
    mol = Chem.AddHs(mol)
    mol_bi = {}
    for r in range(radi + 1):
        mol_fp = rdMolDescriptors.GetMorganFingerprintAsBitVect(mol, radius=r, bitInfo=mol_bi, nBits=2048)
        mol_bi_QC = []
        for i in mol_fp.GetOnBits():
            for j in range(len(mol_bi[i])):
                if mol_bi[i][j][1] == r:
                    mol_bi_QC.append(i)
                    break
        if r == 0:
            for i in mol_bi_QC:
                formula[i] = len([k for k in mol_bi[i] if k[1] == 0])
        else:
            for i in mol_bi_QC:
                binary[(2048 * (r - 1)) + i] = len([k for k in mol_bi[i] if k[1] == r])
    return formula, binary


def vote_classification(n_path, n_class, n_super, pred_class, pred_super, path_from_class, path_from_superclass, index):
    """Copie de prediction_voting.vote_classification (sans isglycoside). Renvoie (voies, superclasses, classes)."""
    index_class = list(index['Class'].keys())
    index_superclass = list(index['Superclass'].keys())
    index_pathway = list(index['Pathway'].keys())
    path_for_vote = n_path + path_from_class + path_from_superclass
    path = list(set([k for k in path_for_vote if path_for_vote.count(k) == 3]))
    if path == []:
        path = list(set([k for k in path_for_vote if path_for_vote.count(k) == 2]))
    if path == []:
        return [index_pathway[w] for w in n_path], [], []
    CH, SH = index['Class_hierarchy'], index['Super_hierarchy']
    sh = lambda l: SH.get(str(l), {'Pathway': []})['Pathway']
    if set(n_path) & set(path) != set():
        if set(path) & set(path_from_superclass) != set():
            n_super = [l for l in n_super if set(path) & set(sh(l)) != set()]
            if n_super == []:
                n_class = [m for m in n_class if set(path) & set(CH[str(m)]['Pathway']) != set()]
                n_super = list(set(itertools.chain.from_iterable(CH[str(n)]['Superclass'] for n in n_class)))
            elif len(n_super) > 1:
                n_class = [u for u in n_class if set(path) & set(CH[str(u)]['Pathway']) != set()]
                if n_class != []:
                    n_super = list(set(itertools.chain.from_iterable(CH[str(v)]['Superclass'] for v in n_class)))
                elif len(path) == 1:
                    n_super = [int(np.argmax(pred_super))]
                    n_class = [m for m in [int(np.argmax(pred_class))] if set(n_super) & set(CH[str(m)]['Superclass']) != set()]
            else:
                n_class = [o for o in n_class if set(n_super) & set(CH[str(o)]['Superclass']) != set()]
                if n_class == []:
                    n_class = [m for m in [int(np.argmax(pred_class))] if set(n_super) & set(CH[str(m)]['Superclass']) != set()]
        else:
            n_class = [p for p in n_class if set(path) & set(CH[str(p)]['Pathway']) != set()]
            n_super = list(set(itertools.chain.from_iterable(CH[str(q)]['Superclass'] for q in n_class)))
    else:
        n_super = [l for l in n_super if set(path) & set(sh(l)) != set()]
        if n_super == []:
            n_class = [m for m in n_class if set(path) & set(CH[str(m)]['Pathway']) != set()]
            n_super = list(set(itertools.chain.from_iterable(CH[str(n)]['Superclass'] for n in n_class)))
        elif len(n_super) > 1:
            n_class = [u for u in n_class if set(path) & set(CH[str(u)]['Pathway']) != set()]
            n_super = list(set(itertools.chain.from_iterable(CH[str(v)]['Superclass'] for v in n_class)))
        else:
            n_class = [o for o in n_class if set(path) & set(CH[str(o)]['Pathway']) != set()]
            n_super = list(set(itertools.chain.from_iterable(CH[str(v)]['Superclass'] for v in n_class)))
    return [index_pathway[r] for r in path], [index_superclass[s] for s in n_super], [index_class[t] for t in n_class]


def cell(values):
    """Cellule FragHub : valeurs triées par ordre alphabétique (le vote officiel passe par des ensembles, dont
    l'ordre change d'un lancement à l'autre), jointes par « | » ; rien -> « NOT FOUND »."""
    return "|".join(sorted(values)) if values else NOT_FOUND


class NPClassifier:
    """Les trois réseaux (Keras 2.4, 2020) relus avec h5py et exécutés en numpy : pas de dépendance à Keras ni à
    TensorFlow. L'empreinte doit venir du hachage Morgan d'avant RDKit 2022.03."""
    DENSE = [("dense", "batch_normalization"), ("dense_1", "batch_normalization_1"), ("dense_2", "batch_normalization_2")]

    def __init__(self):
        import h5py
        with open(os.path.join(MODELS, "index_v1.json"), encoding="utf-8") as h:
            self.index = json.load(h)
        self.weights = {}
        for n in ("pathway", "superclass", "class"):
            f = h5py.File(os.path.join(MODELS, "NP_classifier_%s_V1.hdf5" % n), "r")["model_weights"]
            g = lambda p: f[p][...].astype(np.float32)
            w = {}
            for d, b in self.DENSE:
                w[d] = (g("%s/%s/kernel:0" % (d, d)), g("%s/%s/bias:0" % (d, d)))
                w[b] = tuple(g("%s/%s/%s:0" % (b, b, k)) for k in ("gamma", "beta", "moving_mean", "moving_variance"))
            w["dense_3"] = (g("dense_3/dense_3/kernel:0"), g("dense_3/dense_3/bias:0"))
            w["output"] = (g("output/output/kernel:0"), g("output/output/bias:0"))
            self.weights[n] = w

    def predict(self, name, x):
        w = self.weights[name]
        for d, b in self.DENSE:
            k, bias = w[d]; x = np.maximum(0, x @ k + bias)
            gamma, beta, mean, var = w[b]; x = gamma * (x - mean) / np.sqrt(var + 1e-3) + beta
        k, bias = w["dense_3"]; x = np.maximum(0, x @ k + bias)
        k, bias = w["output"]
        # exp déborde en float32 pour les sorties très négatives : 1 / (1 + inf) = 0, c'est le bon résultat ;
        # on masque seulement l'avertissement de numpy.
        with np.errstate(over="ignore"):
            return 1 / (1 + np.exp(-(x @ k + bias)))

    def classify_batch(self, x):
        """x : empreintes (n, 6144) en float32. Renvoie n triplets (voie, superclasse, classe) au format FragHub."""
        preds = {n: self.predict(n, x) for n in ("pathway", "superclass", "class")}
        out = []
        for row in range(x.shape[0]):
            ps, pc, pp = preds["superclass"][row], preds["class"][row], preds["pathway"][row]
            n_super = list(np.where(ps >= 0.3)[0]); n_class = list(np.where(pc >= 0.1)[0]); n_path = list(np.where(pp >= 0.5)[0])
            pfs = sorted(set(itertools.chain.from_iterable(self.index['Super_hierarchy'].get(str(j), {'Pathway': []})['Pathway'] for j in n_super)))
            pfc = sorted(set(itertools.chain.from_iterable(self.index['Class_hierarchy'][str(j)]['Pathway'] for j in n_class)))
            path, sup, cls = vote_classification(n_path, n_class, n_super, pc, ps, pfc, pfs, self.index)
            out.append((cell(path), cell(sup), cell(cls)))
        return out


# ----------------------------------------------------------------------------------------------------------------
# Empreintes en parallèle. Une empreinte a 6 144 cases presque toutes à zéro : les processus renvoient seulement
# les cases non nulles (positions et valeurs), le processus principal reconstruit le même vecteur.
# ----------------------------------------------------------------------------------------------------------------

def _worker_init():
    from rdkit import RDLogger
    RDLogger.DisableLog("rdApp.*")


def fingerprints_sparse(smiles_chunk):
    """Pour chaque SMILES : (positions, valeurs) des cases non nulles de [formule (2048), rayons 1-2 (4096)],
    ou None si le SMILES est illisible."""
    out = []
    for s in smiles_chunk:
        try:
            fp = fingerprint(s)
        except Exception:
            fp = None
        if fp is None:
            out.append(None)
            continue
        vec = np.concatenate([fp[0], fp[1]])
        pos = np.nonzero(vec)[0]
        out.append((pos.astype(np.uint16), vec[pos].astype(np.int32)))
    return out


def dense_batch(sparse):
    x = np.zeros((len(sparse), 6144), dtype=np.float32)
    for i, (pos, val) in enumerate(sparse):
        x[i, pos] = val
    return x


# ----------------------------------------------------------------------------------------------------------------
# COCONUT
# ----------------------------------------------------------------------------------------------------------------

def split_names(value, known):
    """Sépare une cellule COCONUT en noms de l'index. Certains noms contiennent « , » (« Carotenoids (C40, β-β) ») :
    on recolle les morceaux jusqu'à retrouver un nom connu. None si la cellule ne se découpe pas en noms connus."""
    value = (value or "").strip()
    if not value:
        return []
    if value in known:
        return [value]
    for sep in ("|", ", ", "; ", ","):
        if sep not in value:
            continue
        parts, names, i = value.split(sep), [], 0
        while i < len(parts):
            for j in range(len(parts), i, -1):
                cand = sep.join(parts[i:j]).strip()
                if cand in known:
                    names.append(cand)
                    i = j
                    break
            else:
                names = None
                break
        if names:
            return names
    return None


def read_coconut(path, index):
    """InChIKey complète -> (voie, superclasse, classe) au format FragHub, pour les molécules de COCONUT qui ont au
    moins une voie. Renvoie aussi le nombre de cellules dont le contenu n'est pas dans l'index (gardées telles quelles)."""
    csv.field_size_limit(2 ** 31 - 1)
    known = [set(index[k].keys()) for k in ("Pathway", "Superclass", "Class")]
    cols = ("np_classifier_pathway", "np_classifier_superclass", "np_classifier_class")
    coco, unknown = {}, 0
    with open(path, newline="", encoding="utf-8") as h:
        for r in csv.DictReader(h):
            key = (r.get("standard_inchi_key") or "").strip()
            if not key or not (r.get(cols[0]) or "").strip():
                continue
            triple = []
            for c, kn in zip(cols, known):
                names = split_names(r.get(c), kn)
                if names is None:
                    unknown += 1
                    triple.append((r.get(c) or "").strip())
                else:
                    triple.append(cell(names))
            coco[key] = tuple(triple)
    return coco, unknown


# ----------------------------------------------------------------------------------------------------------------
# Base
# ----------------------------------------------------------------------------------------------------------------

def check_rdkit():
    import rdkit
    parts = rdkit.__version__.split(".")
    try:
        version = (int(parts[0]), int(parts[1]))
    except (ValueError, IndexError):
        sys.exit("Version de RDKit illisible : %s" % rdkit.__version__)
    if version >= (2022, 3):
        sys.exit("RDKit %s : le hachage des empreintes Morgan a changé dans RDKit 2022.03, les modèles NP-Classifier V1 "
                 "donneraient des classes fausses.\nLancer ce script avec l'environnement dédié (RDKit 2020.03.6), voir "
                 "l'en-tête du script." % rdkit.__version__)
    return rdkit.__version__


def keep_awake(on):
    """Empêche la mise en veille de Windows pendant le calcul (comme renormalize_db.py)."""
    if sys.platform == 'win32':
        import ctypes
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED = 0x80000000, 0x00000001
        ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | (ES_SYSTEM_REQUIRED if on else 0))


def dictionary_column(values, field):
    """Colonne dictionnaire du même type que la colonne d'origine (index élargi en uint32 s'il est trop petit)."""
    import pyarrow as pa
    arr = np.array([v if v is not None else "" for v in values], dtype=object)
    mask = np.array([v is None for v in values], dtype=bool)
    uniq, inverse = np.unique(arr, return_inverse=True)
    index_type = field.type.index_type if pa.types.is_dictionary(field.type) else pa.uint32()
    limits = {pa.int8(): 127, pa.int16(): 32767, pa.int32(): 2 ** 31 - 1, pa.uint8(): 255, pa.uint16(): 65535}
    if len(uniq) > limits.get(index_type, 2 ** 32 - 1):
        index_type = pa.uint32()
    indices = pa.array(inverse.astype(np.int64), type=index_type, mask=mask if mask.any() else None)
    return pa.DictionaryArray.from_arrays(indices, pa.array(list(uniq), type=pa.string()))


def read_checkpoint(path):
    """Fichier de reprise : ligne du parquet -> (voie, superclasse, classe). Une dernière ligne coupée par un arrêt
    brutal (pas de marque de fin) est ignorée."""
    done = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as h:
            for line in h:
                f = line.rstrip("\n").split("\t")
                if len(f) == 5 and f[4] == "." and f[0].isdigit():
                    done[int(f[0])] = (f[1], f[2], f[3])
    return done


def process_file(name, src, dst, clf, coco, pool, limit, log, sample):
    """Traite un fichier. Renvoie les compteurs ; n'écrit le parquet que si limit est None."""
    import pyarrow as pa
    import pyarrow.parquet as pq
    t0 = time.time()
    table = pq.read_table(src)
    n = table.num_rows if limit is None else min(limit, table.num_rows)
    keys = table.column("INCHIKEY").to_pylist()[:n]
    smiles = table.column("SMILES").to_pylist()[:n]
    old = [table.column(c).to_pylist()[:n] for c in COLUMNS]
    usable = [i for i in range(n) if isinstance(smiles[i], str) and smiles[i].strip()]
    stats = {"lignes": n, "coconut": 0, "modele": 0, "gardees": n - len(usable), "illisibles": 0,
             "changements": [0, 0, 0], "accord_voie": 0, "accord_3": 0, "accord_n": 0}
    if not usable:
        if limit is None:
            shutil.copyfile(src, dst + ".tmp")
            os.replace(dst + ".tmp", dst)
        log("%s : aucun SMILES, fichier recopié tel quel (%d lignes)" % (name, n))
        return stats

    new = [list(col) for col in old]
    to_model = []
    for i in usable:
        ref = coco.get(keys[i])
        if ref is not None and limit is None:
            for c in range(3):
                new[c][i] = ref[c]
            stats["coconut"] += 1
        else:
            # En essai, le modèle est aussi lancé sur les molécules de COCONUT, pour mesurer l'accord
            to_model.append(i)

    # Reprise : classes du modèle déjà calculées lors d'un lancement précédent (jamais en essai)
    checkpoint_path = dst + ".reprise"
    resumed = read_checkpoint(checkpoint_path) if limit is None else {}
    if resumed:
        for i in to_model:
            if i in resumed:
                for c in range(3):
                    new[c][i] = resumed[i][c]
        stats["modele"] += sum(1 for i in to_model if i in resumed)
        to_model = [i for i in to_model if i not in resumed]
        log("%s : reprise, %d molécules déjà classées, %d à faire" % (name, len(resumed), len(to_model)))
    checkpoint = open(checkpoint_path, "a", encoding="utf-8") if limit is None else None

    done = 0
    for b in range(0, len(to_model), BLOCK):
        rows = to_model[b:b + BLOCK]
        chunks = [[smiles[i] for i in rows[c:c + CHUNK]] for c in range(0, len(rows), CHUNK)]
        sparse = list(itertools.chain.from_iterable(pool.imap(fingerprints_sparse, chunks)))
        readable = [(i, fp) for i, fp in zip(rows, sparse) if fp is not None]
        stats["illisibles"] += len(rows) - len(readable)
        for k in range(0, len(readable), BATCH):
            part = readable[k:k + BATCH]
            for (i, _), res in zip(part, clf.classify_batch(dense_batch([fp for _, fp in part]))):
                ref = coco.get(keys[i])
                if ref is not None:
                    # Essai seulement : accord du modèle avec COCONUT, puis classes COCONUT comme dans le vrai run
                    stats["accord_n"] += 1
                    stats["accord_voie"] += res[0] == ref[0]
                    stats["accord_3"] += res == ref
                    res = ref
                    stats["coconut"] += 1
                else:
                    stats["modele"] += 1
                for c in range(3):
                    new[c][i] = res[c]
        if checkpoint is not None:
            # Bloc terminé : ses classes sont ajoutées au fichier de reprise et écrites sur le disque
            checkpoint.write("".join("%d\t%s\t%s\t%s\t.\n" % (i, new[0][i], new[1][i], new[2][i]) for i, _ in readable))
            checkpoint.flush()
            os.fsync(checkpoint.fileno())
        done += len(rows)
        print("\r  %s : %d/%d molécules classées par le modèle (%.0f s)   " % (name, done, len(to_model), time.time() - t0), end="", flush=True)
    print()
    if checkpoint is not None:
        checkpoint.close()
    stats["gardees"] += stats["illisibles"]

    noted = 0
    for i in usable:
        changed = [new[c][i] != old[c][i] for c in range(3)]
        for c in range(3):
            stats["changements"][c] += changed[c]
        if any(changed) and noted < SAMPLE_PER_FILE:
            sample.writerow([name, keys[i], "COCONUT" if keys[i] in coco else "modèle"] + [old[c][i] for c in range(3)] + [new[c][i] for c in range(3)])
            noted += 1

    if limit is None:
        for c, column in enumerate(COLUMNS):
            pos = table.schema.get_field_index(column)
            table = table.set_column(pos, table.schema.field(pos).name, dictionary_column(new[c], table.schema.field(pos)))
        # Même écriture que l'original : ZSTD, un seul groupe de lignes
        pq.write_table(table, dst + ".tmp", compression="zstd", row_group_size=max(table.num_rows, 1))
        # Relecture de contrôle. Le fichier est ouvert et REFERMÉ ici : sous Windows, un fichier encore ouvert
        # ne peut pas être renommé (pq.ParquetFile(chemin) le gardait ouvert).
        with open(dst + ".tmp", "rb") as handle:
            written = pq.ParquetFile(handle).metadata.num_rows
        if written != table.num_rows:
            raise RuntimeError("%s : %d lignes écrites au lieu de %d" % (name, written, table.num_rows))
        os.replace(dst + ".tmp", dst)
        os.remove(checkpoint_path)
    log("%s : %d lignes ; COCONUT %d, modèle %d, gardées %d (dont %d SMILES illisibles) ; changements voie %d, superclasse %d, classe %d (%.0f s)"
        % (name, n, stats["coconut"], stats["modele"], stats["gardees"], stats["illisibles"],
           stats["changements"][0], stats["changements"][1], stats["changements"][2], time.time() - t0))
    return stats


def main():
    ap = argparse.ArgumentParser(description="Recalcule les classes NPClassifier de la base interne.")
    ap.add_argument("--input", default=os.path.join(ROOT, "datas", "internal_databases.parquet"))
    ap.add_argument("--output", default=os.path.join(ROOT, "scratch", "npclassifier_db", "internal_databases.parquet"))
    ap.add_argument("--coconut", required=True, help="coconut_csv-11-2026.csv")
    ap.add_argument("--only", nargs="*")
    ap.add_argument("--procs", type=int, default=os.cpu_count() or 1)
    ap.add_argument("--max", type=int, default=None, help="essai : N premières lignes par fichier, aucun parquet écrit")
    a = ap.parse_args()

    version = check_rdkit()
    if os.path.abspath(a.input) == os.path.abspath(a.output):
        sys.exit("La sortie doit être un nouveau dossier : la base d'origine n'est jamais modifiée.")
    os.makedirs(a.output, exist_ok=True)
    journal = open(os.path.join(a.output, "_npclassifier_journal.txt"), "a", encoding="utf-8")

    def log(message):
        print(message)
        journal.write(time.strftime("%Y-%m-%d %H:%M:%S ") + message + "\n")
        journal.flush()

    log("RDKit %s ; entrée %s ; sortie %s%s" % (version, a.input, a.output, " ; ESSAI (--max %d), aucun parquet écrit" % a.max if a.max else ""))
    clf = NPClassifier()
    t0 = time.time()
    coco, unknown = read_coconut(a.coconut, clf.index)
    log("COCONUT : %d molécules avec des classes (%d cellules hors index gardées telles quelles), lu en %.0f s" % (len(coco), unknown, time.time() - t0))

    names = sorted(f for f in os.listdir(a.input) if f.endswith(".parquet"))
    if a.only:
        missing = [f for f in a.only if f not in names]
        if missing:
            sys.exit("Fichiers absents de %s : %s" % (a.input, ", ".join(missing)))
        names = [f for f in names if f in a.only]

    sample_path = os.path.join(a.output, "_npclassifier_changements.tsv")
    new_sample = not os.path.exists(sample_path)
    sample_file = open(sample_path, "a", newline="", encoding="utf-8")
    sample = csv.writer(sample_file, delimiter="\t", lineterminator="\n")
    if new_sample:
        sample.writerow(["fichier", "INCHIKEY", "source"] + ["avant_" + c for c in COLUMNS] + ["apres_" + c for c in COLUMNS])

    total = {"lignes": 0, "coconut": 0, "modele": 0, "gardees": 0, "illisibles": 0, "changements": [0, 0, 0],
             "accord_voie": 0, "accord_3": 0, "accord_n": 0}
    keep_awake(True)
    pool = multiprocessing.Pool(max(1, a.procs), initializer=_worker_init)
    try:
        for name in names:
            dst = os.path.join(a.output, name)
            if a.max is None and os.path.exists(dst):
                log("%s : déjà écrit, sauté" % name)
                continue
            st = process_file(name, os.path.join(a.input, name), dst, clf, coco, pool, a.max, log, sample)
            sample_file.flush()
            for k, v in st.items():
                if k == "changements":
                    total[k] = [x + y for x, y in zip(total[k], v)]
                else:
                    total[k] += v
    finally:
        pool.terminate()
        keep_awake(False)
        sample_file.close()
    log("TOTAL : %d lignes ; COCONUT %d, modèle %d, gardées %d (dont %d SMILES illisibles) ; changements voie %d, superclasse %d, classe %d ; %.0f min"
        % (total["lignes"], total["coconut"], total["modele"], total["gardees"], total["illisibles"],
           total["changements"][0], total["changements"][1], total["changements"][2], (time.time() - t0) / 60))
    if total["accord_n"]:
        log("ESSAI, accord du modèle avec COCONUT sur %d molécules : voie identique %.1f %%, trois niveaux identiques %.1f %%"
            % (total["accord_n"], 100.0 * total["accord_voie"] / total["accord_n"], 100.0 * total["accord_3"] / total["accord_n"]))
    journal.close()


if __name__ == "__main__":
    main()
