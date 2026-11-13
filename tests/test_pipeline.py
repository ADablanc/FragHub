# -*- coding: utf-8 -*-
"""Tests du moteur complet (fraghub_rust.main_orchestrator) sur de très petits fichiers écrits par le test.

Le moteur est lancé comme le fait FragHub.py, de la lecture des fichiers à l'écriture des sorties, mais avec une
base interne de DEUX molécules fabriquée ici (caféine, quercétine) : pas besoin de la vraie base (1 Go), et un run
prend moins d'une seconde. Chaque test vérifie un résultat précis : tel spectre est écrit dans telle bibliothèque,
supprimé avec tel motif, complété avec telle valeur.

Ces tests couvrent ce que le test de bout en bout (test_bout_en_bout.py) ne peut pas garantir avec son jeu d'entrée :
tous les motifs de suppression, les douze bibliothèques de sortie, le mode mise à jour, l'arrêt en cours de run, les
options de sortie et les filtres désactivés.

Ils utilisent le moteur COMPILÉ installé dans .venv, pas les sources Rust : après une modification du Rust, il faut
recompiler (python build_backend.py --rust-only), sinon ces tests s'arrêtent en le disant (voir conftest.py).
"""
import csv
import hashlib
import json
import os
import shutil
import zlib

import pytest

pa = pytest.importorskip("pyarrow", reason="pyarrow sert à fabriquer la mini-base (uv sync)")
import pyarrow.parquet as pq  # noqa: E402

fraghub_rust = pytest.importorskip("fraghub_rust", reason="moteur Rust non installé (python build_backend.py --rust-only)")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ---------------------------------------------------------------------------------------------------------------
# Molécules d'essai. Les fragments sont de vrais fragments (formule possible à partir du précurseur, 4 décimales) :
# ils passent les contrôles de l'étape « Mass & Adduct Correction » (mz_correction.rs).
# ---------------------------------------------------------------------------------------------------------------
CAFFEINE = dict(
    name="Caffeine", inchikey="RYYVLZVUVIJVGH-UHFFFAOYSA-N",
    inchi="InChI=1S/C8H10N4O2/c1-10-4-9-6-5(10)7(13)12(3)8(14)11(6)2/h4H,1-3H3",
    smiles="CN1C=NC2=C1C(=O)N(C(=O)N2C)C", adduct="[M+H]+", mz="195.0877", mode="P",
    peaks=[(56.0495, 20), (69.0447, 15), (83.0604, 10), (110.0713, 60), (138.0662, 100)])
QUERCETIN = dict(
    name="Quercetin", inchikey="REFJWTPEDVJJIY-UHFFFAOYSA-N",
    inchi="InChI=1S/C15H10O7/c16-7-4-10(19)12-11(5-7)22-15(14(21)13(12)20)6-1-2-8(17)9(18)3-6/h1-5,16-19,21H",
    smiles="C1=CC(=C(C=C1C2=C(C(=O)C3=C(C=C(C=C3O2)O)O)O)O)O", adduct="[M-H]-", mz="301.0354", mode="N",
    peaks=[(107.0139, 12), (121.0295, 18), (151.0037, 100), (178.9986, 45), (273.0405, 8)])
# Absente de la mini-base
PARACETAMOL = dict(
    name="Paracetamol", inchikey="RZVAJINKPMORJF-UHFFFAOYSA-N",
    inchi="InChI=1S/C8H9NO2/c1-6(10)9-7-2-4-8(11)5-3-7/h2-5,11H,1H3,(H,9,10)",
    smiles="CC(=O)NC1=CC=C(O)C=C1", adduct="[M+H]+", mz="152.0706", mode="P",
    peaks=[(65.0386, 14), (93.0335, 22), (110.0600, 100)])

# Mini-base : mêmes colonnes que la vraie (datas/internal_databases.parquet)
BASE_ROWS = [
    dict(INCHIKEY=CAFFEINE["inchikey"], INCHI=CAFFEINE["inchi"], SMILES="CN1C=NC2=C1C(=O)N(C)C(=O)N2C",
         FORMULA="C8H10N4O2", NAME="Caffeine (base)", EXACTMASS=194.080376, AVERAGEMASS=194.194,
         CLASSYFIRE_SUPERCLASS="Organoheterocyclic compounds", CLASSYFIRE_CLASS="Imidazopyrimidines",
         CLASSYFIRE_SUBCLASS="Purines and purine derivatives", NPCLASS_PATHWAY="Alkaloids",
         NPCLASS_SUPERCLASS="Pseudoalkaloids", NPCLASS_CLASS="Purine alkaloids"),
    dict(INCHIKEY=QUERCETIN["inchikey"], INCHI=QUERCETIN["inchi"], SMILES=QUERCETIN["smiles"],
         FORMULA="C15H10O7", NAME="Quercetin (base)", EXACTMASS=302.042653, AVERAGEMASS=302.238,
         CLASSYFIRE_SUPERCLASS="Phenylpropanoids and polyketides", CLASSYFIRE_CLASS="Flavonoids",
         CLASSYFIRE_SUBCLASS="Flavones", NPCLASS_PATHWAY="Shikimates and Phenylpropanoids",
         NPCLASS_SUPERCLASS="Flavonoids", NPCLASS_CLASS="Flavonols"),
]
CLASS_COLUMNS = ["CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS",
                 "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS"]

# Paramètres par défaut de FragHub.py (mode ligne de commande), sans le calcul de novo ni la sortie mzSpecLib
DEFAULTS = dict(
    input_db_names={}, normalize_intensity=1.0, remove_peak_above_precursormz=1.0, check_minimum_peak_requiered=1.0,
    check_minimum_peak_requiered_n_peaks=3.0, reduce_peak_list=1.0, reduce_peak_list_max_peaks=500.0,
    remove_spectrum_under_entropy_score=1.0, remove_spectrum_under_entropy_score_value=0.5, keep_mz_in_range=1.0,
    keep_mz_in_range_from_mz=50.0, keep_mz_in_range_to_mz=2000.0, check_minimum_of_high_peaks_requiered=1.0,
    check_minimum_of_high_peaks_requiered_intensity_percent=5.0, check_minimum_of_high_peaks_requiered_no_peaks=2.0,
    calculate_de_novo=0.0, de_novo_ppm_tolerance=10.0, csv=1.0, msp=1.0, json=1.0, mzspeclib_json=0.0,
    reset_updates=0.0)


# ---------------------------------------------------------------------------------------------------------------
# Écriture des spectres d'essai dans chaque format d'entrée
# ---------------------------------------------------------------------------------------------------------------
def own_peaks(mol, spectrum_id):
    """Pics de la molécule avec un m/z du premier pic (peu intense) propre à chaque identifiant : deux spectres
    d'essai n'ont jamais les mêmes pics, donc jamais le même SPLASH (sinon l'un serait supprimé comme doublon de
    l'autre). Le m/z, et non l'intensité : le SPLASH arrondit les intensités à 1 % du pic le plus intense."""
    (mz, intensity), rest = mol["peaks"][0], mol["peaks"][1:]
    return [(round(int(mz) + (zlib.crc32(spectrum_id.encode()) % 9000 + 500) / 10000, 4), intensity)] + rest


def peaks_text(peaks):
    return "".join("%s %s\n" % p for p in peaks)


def msp(spectrum_id, mol=CAFFEINE, peaks=None, **fields):
    """Un spectre MSP. `fields` remplace ou ajoute des lignes ; une valeur None retire la ligne. `peaks` : liste de
    couples (m/z, intensité), ou texte déjà écrit ; par défaut les pics de la molécule (own_peaks)."""
    lines = {"Name": mol["name"], "DB#": spectrum_id, "Precursor_type": mol["adduct"], "PrecursorMZ": mol["mz"],
             "Ion_mode": mol["mode"], "InChIKey": mol["inchikey"], "SMILES": mol["smiles"],
             "Instrument_type": "LC-ESI-QTOF"}
    lines.update(fields)
    peaks = own_peaks(mol, spectrum_id) if peaks is None else peaks
    text = "".join("%s: %s\n" % (k, v) for k, v in lines.items() if v is not None)
    if isinstance(peaks, str):
        return text + "Num Peaks: %d\n" % peaks.count("\n") + peaks + "\n"
    return text + "Num Peaks: %d\n" % len(peaks) + peaks_text(peaks) + "\n"


def mgf(spectrum_id, mol=CAFFEINE, peaks=None, **fields):
    """Un spectre MGF (bloc BEGIN IONS / END IONS)."""
    lines = {"NAME": mol["name"], "SPECTRUMID": spectrum_id, "ADDUCT": mol["adduct"], "PEPMASS": mol["mz"],
             "IONMODE": "Positive" if mol["mode"] == "P" else "Negative", "INCHIKEY": mol["inchikey"],
             "SMILES": mol["smiles"], "SOURCE_INSTRUMENT": "LC-ESI-QTOF"}
    lines.update(fields)
    peaks = own_peaks(mol, spectrum_id) if peaks is None else peaks
    body = peaks if isinstance(peaks, str) else peaks_text(peaks)
    return "BEGIN IONS\n" + "".join("%s=%s\n" % (k, v) for k, v in lines.items() if v is not None) + body + "END IONS\n\n"


def json_object(spectrum_id, mol=CAFFEINE, peaks=None, peaks_key="peaks", **fields):
    """Un spectre JSON simple. `peaks` : liste de couples, ou texte (JSON GNPS : les pics sont un texte dans
    « peaks_json »)."""
    obj = {"compound_name": mol["name"], "spectrum_id": spectrum_id, "adduct": mol["adduct"],
           "precursor_mz": mol["mz"], "ion_mode": "Positive" if mol["mode"] == "P" else "Negative",
           "inchikey": mol["inchikey"], "smiles": mol["smiles"], "instrument_type": "LC-ESI-QTOF"}
    obj.update(fields)
    obj = {k: v for k, v in obj.items() if v is not None}
    obj[peaks_key] = [list(p) for p in own_peaks(mol, spectrum_id)] if peaks is None else peaks
    return obj


def csv_file(spectrum_id, mol=CAFFEINE, peaks=None):
    """Un fichier CSV d'entrée d'un spectre (séparateur « ; », pics « m/z intensité » séparés par « ; »)."""
    row = {"NAME": mol["name"], "SPECTRUMID": spectrum_id, "PRECURSORTYPE": mol["adduct"], "PRECURSORMZ": mol["mz"],
           "IONMODE": "positive" if mol["mode"] == "P" else "negative", "INCHIKEY": mol["inchikey"],
           "SMILES": mol["smiles"], "INSTRUMENTTYPE": "LC-ESI-QTOF"}
    peaks = own_peaks(mol, spectrum_id) if peaks is None else peaks
    row["PEAKS_LIST"] = peaks if isinstance(peaks, str) else ";".join("%s %s" % p for p in peaks)
    return ";".join(row) + "\n" + ";".join('"%s"' % v for v in row.values()) + "\n"


def write(folder, name, text):
    os.makedirs(str(folder), exist_ok=True)
    path = os.path.join(str(folder), name)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    return path


# ---------------------------------------------------------------------------------------------------------------
# Lancement d'un run et lecture de ses sorties
# ---------------------------------------------------------------------------------------------------------------
class Run:
    """Résultat d'un run : messages du moteur, erreur éventuelle, spectres gardés et supprimés lus dans les sorties."""

    def __init__(self, out, messages, error):
        self.out, self.messages, self.error = out, messages, error

    def files(self):
        """Fichiers de sortie (chemins relatifs avec « / »), sans le rapport ni updates.json."""
        found = []
        for root, _, names in os.walk(self.out):
            found += [os.path.relpath(os.path.join(root, n), self.out).replace(os.sep, "/") for n in names]
        return sorted(f for f in found if not (f.startswith("report_") or f == "updates.json"))

    def snapshot(self):
        """{fichier: empreinte} de TOUT le dossier de sortie, pour vérifier qu'il n'a pas changé."""
        content = {}
        for root, _, names in os.walk(self.out):
            for n in names:
                path = os.path.join(root, n)
                with open(path, "rb") as f:
                    content[os.path.relpath(path, self.out)] = hashlib.sha256(f.read()).hexdigest()
        return content

    def _rows(self, path, delimiter):
        csv.field_size_limit(2 ** 31 - 1)
        with open(os.path.join(self.out, path), encoding="utf-8", newline="") as h:
            return list(csv.DictReader(h, delimiter=delimiter))

    @property
    def kept(self):
        """{SPECTRUMID: ligne du CSV de sortie}, avec la bibliothèque dans « LIB » (ex. « POS_LC »)."""
        rows = {}
        for f in self.files():
            if f.startswith("CSV/"):
                for r in self._rows(f, ";"):
                    assert r["SPECTRUMID"] not in rows, "spectre écrit deux fois : %s" % r["SPECTRUMID"]
                    rows[r["SPECTRUMID"]] = dict(r, LIB=os.path.basename(f)[:-4])
        return rows

    @property
    def deleted(self):
        """{SPECTRUMID: [lignes de DELETED_SPECTRUMS]}, avec le nom du fichier (sans .csv) dans « FILE »."""
        rows = {}
        for f in self.files():
            if f.startswith("DELETED_SPECTRUMS/"):
                for r in self._rows(f, "\t"):
                    rows.setdefault(r["SPECTRUMID"], []).append(dict(r, FILE=os.path.basename(f)[:-4]))
        return rows

    def deletion_file(self, spectrum_id):
        """Fichier de DELETED_SPECTRUMS (sans .csv) où le spectre est écrit ; il ne doit y être qu'une fois."""
        rows = self.deleted.get(spectrum_id, [])
        assert len(rows) == 1, "%s : écrit %d fois dans DELETED_SPECTRUMS" % (spectrum_id, len(rows))
        return rows[0]["FILE"]

    def said(self, start):
        """Le moteur a-t-il envoyé un message qui commence par `start` ?"""
        return any(str(m[0]).startswith(start) for m in self.messages if m)


@pytest.fixture(scope="module")
def engine(tmp_path_factory, moteur_a_jour):
    """Moteur Rust chargé avec la mini-base et les vraies tables du dépôt (adduits, clés, arbre des instruments)."""
    datas = tmp_path_factory.mktemp("mini_base") / "datas"
    (datas / "internal_databases.parquet").mkdir(parents=True)
    for name in ("adduct_to_convert.csv", "key_to_convert.csv", "instruments_tree.json"):
        shutil.copyfile(os.path.join(ROOT, "datas", name), datas / name)
    pq.write_table(pa.Table.from_pylist(BASE_ROWS), datas / "internal_databases.parquet" / "part_0000.parquet")
    fraghub_rust.load_internal_databases(str(datas.parent))
    return fraghub_rust


def run(engine, out, inputs, stop_flag=None, mzspeclib_sink=None, **params):
    """Lance le moteur sur `inputs` (chemins de fichiers). `params` remplace les paramètres par défaut."""
    p = dict(DEFAULTS, input_directory=[str(i) for i in inputs], output_directory=str(out))
    p.update(params)
    messages, error = [], None
    try:
        engine.main_orchestrator(p, deletion_callback=lambda *a: messages.append(a),
                                 completion_callback=lambda *a: messages.append(a), stop_flag=stop_flag,
                                 mzspeclib_sink=mzspeclib_sink)
    except Exception as e:  # noqa: BLE001 — rendue au test, qui dit si elle est attendue
        error = e
    return Run(str(out), messages, error)


def run_ok(engine, out, inputs, **params):
    result = run(engine, out, inputs, **params)
    assert result.error is None, "le moteur a échoué : %r" % result.error
    return result


# ---------------------------------------------------------------------------------------------------------------
# Lecture : le même spectre dans tous les formats d'entrée
# ---------------------------------------------------------------------------------------------------------------
SAME_FIELDS = ["SPLASH", "PRECURSORMZ", "PRECURSORTYPE", "IONMODE", "INSTRUMENTTYPE", "INCHIKEY", "IONIZATION",
               "RESOLUTION", "MSLEVEL"]


def one_kept_others_duplicates(result, ids):
    """Les spectres `ids` ont les mêmes pics et la même molécule : un seul est gardé, les autres sont supprimés
    comme doublons, avec les mêmes valeurs lues. Renvoie la ligne gardée."""
    kept = [i for i in ids if i in result.kept]
    assert len(kept) == 1, "un seul spectre gardé attendu, obtenu : %s" % kept
    row = result.kept[kept[0]]
    for i in ids:
        if i != kept[0]:
            assert result.deletion_file(i) == "duplicatas_removed", i
            other = result.deleted[i][0]
            assert {c: other[c] for c in SAME_FIELDS} == {c: row[c] for c in SAME_FIELDS}, i
    return row


def test_meme_spectre_dans_tous_les_formats(engine, tmp_path):
    """MSP, MGF, JSON (tableau), JSONL, CSV et JSON GNPS (pics dans un texte) : mêmes pics lus (même SPLASH) et
    mêmes champs normalisés, quel que soit le format."""
    p = CAFFEINE["peaks"]
    inputs = [
        write(tmp_path, "a.msp", msp("F_MSP", peaks=p)),
        write(tmp_path, "b.mgf", mgf("F_MGF", peaks=p)),
        write(tmp_path, "c.json", json.dumps([json_object("F_JSON", peaks=[list(x) for x in p])])),
        write(tmp_path, "d.jsonl", json.dumps(json_object("F_JSONL", peaks=[list(x) for x in p])) + "\n"),
        write(tmp_path, "e.csv", csv_file("F_CSV", peaks=p)),
        write(tmp_path, "f.json", json.dumps([json_object("F_GNPS", peaks=json.dumps([list(x) for x in p]),
                                                          peaks_key="peaks_json")])),
    ]
    result = run_ok(engine, tmp_path / "out", inputs)
    row = one_kept_others_duplicates(result, ["F_MSP", "F_MGF", "F_JSON", "F_JSONL", "F_CSV", "F_GNPS"])
    assert row["LIB"] == "POS_LC"
    # Intensités ramenées à 1 pour le pic le plus intense
    assert row["PEAKS_LIST"] == "56.0495 0.2;69.0447 0.15;83.0604 0.1;110.0713 0.6;138.0662 1"
    assert row["NUM PEAKS"] == "5"
    assert (row["PRECURSORMZ"], row["PRECURSORTYPE"], row["IONMODE"]) == ("195.0877", "[M+H]+", "positive")


def test_pics_en_ecriture_scientifique(engine, tmp_path):
    """Pics écrits « 1.5E+05 », « 6e+1 » (majuscule ou minuscule, avec ou sans signe) : lus comme des nombres dans
    tous les formats. Cas réel : intensité en écriture scientifique dans un spectre GNPS."""
    sci = "5.60495E+01 2.0e1\n6.90447e1 1.5E+01\n83.0604 1E1\n1.100713E2 6e+1\n138.0662 1.0E2\n"
    sci_json = "[[5.60495E+01, 2.0e1], [6.90447e1, 1.5E+01], [83.0604, 1E1], [1.100713E2, 6e+1], [138.0662, 1.0E2]]"
    as_array = json.dumps([json_object("S_JSON", peaks="@")]).replace('"@"', sci_json)   # nombres, pas un texte
    inputs = [
        write(tmp_path, "ref.msp", msp("S_REF", peaks=CAFFEINE["peaks"])),               # écriture ordinaire
        write(tmp_path, "a.msp", msp("S_MSP", peaks=sci)),
        write(tmp_path, "b.mgf", mgf("S_MGF", peaks=sci)),
        write(tmp_path, "c.json", as_array),
        write(tmp_path, "d.json", json.dumps([json_object("S_GNPS", peaks=sci_json, peaks_key="peaks_json")])),
        write(tmp_path, "e.csv", csv_file("S_CSV", peaks=sci.replace("\n", ";"))),
    ]
    result = run_ok(engine, tmp_path / "out", inputs)
    row = one_kept_others_duplicates(result, ["S_REF", "S_MSP", "S_MGF", "S_JSON", "S_GNPS", "S_CSV"])
    assert row["PEAKS_LIST"] == "56.0495 0.2;69.0447 0.15;83.0604 0.1;110.0713 0.6;138.0662 1"


def test_pics_avec_virgule_decimale(engine, tmp_path):
    """Virgule décimale dans le m/z et dans l'intensité (MSP et MGF) : lue comme un point. Avant, la virgule n'était
    acceptée que dans l'intensité : aucun pic n'était lu et le spectre était supprimé (« peaks list is empty »)."""
    comma = "56,0495 20\n69,0447 15,0\n83,0604\t10\n110,0713 60\n138,0662 100,0\n"
    inputs = [
        write(tmp_path, "ref.msp", msp("V_REF", peaks=CAFFEINE["peaks"])),               # écriture avec des points
        write(tmp_path, "a.msp", msp("V_MSP", peaks=comma)),
        write(tmp_path, "b.mgf", mgf("V_MGF", peaks=comma)),
    ]
    result = run_ok(engine, tmp_path / "out", inputs)
    row = one_kept_others_duplicates(result, ["V_REF", "V_MSP", "V_MGF"])
    assert row["PEAKS_LIST"] == "56.0495 0.2;69.0447 0.15;83.0604 0.1;110.0713 0.6;138.0662 1"


# ---------------------------------------------------------------------------------------------------------------
# Suppressions : un spectre par motif, chacun dans le bon fichier de DELETED_SPECTRUMS
# ---------------------------------------------------------------------------------------------------------------
C = CAFFEINE
DELETIONS = {
    "peaks_list_is_empty": [msp("D_NOPEAKS", peaks=[])],
    "no_inchi_smiles_or_inchikey": [msp("D_NOSTRUCT", InChIKey=None, SMILES=None)],
    "suspect_related_structure": [msp("D_SUSPECT", Name="Suspect related to Caffeine with delta m/z -14.016")],
    # m/z du précurseur : pas de SMILES ici, sinon le m/z manquant serait recalculé à partir de la structure
    "precursor_mz_less_than_or_equal_zero": [msp("D_PMZ_NEG", PrecursorMZ="-5", SMILES=None)],
    "precursor_mz_invalid_or_empty": [msp("D_PMZ_TEXTE", PrecursorMZ="abc", SMILES=None),
                                      msp("D_PMZ_ABSENT", PrecursorMZ=None, SMILES=None),
                                      msp("D_PMZ_ZERO", PrecursorMZ="0", SMILES=None)],
    "adduct_empty_or_invalid": [msp("D_SANS_ADDUIT", Precursor_type=None)],
    "wrong_adduct_neg_in_pos": [msp("D_NEG_EN_POS", Precursor_type="[M-H]-")],
    "wrong_adduct_pos_in_neg": [msp("D_POS_EN_NEG", mol=QUERCETIN, Precursor_type="[M+H]+")],
    "ionmode_missing_or_invalid": [msp("D_SANS_MODE", Ion_mode=None, Precursor_type="[M+Foo]")],
    "number_of_peaks_below_threshold": [msp("D_DEUX_PICS", peaks=C["peaks"][-2:])],
    "peaks_empty_after_above_precursor_mz_removal": [msp("D_AU_DESSUS", peaks=[(250.1, 10), (260.2, 30), (270.3, 100)])],
    "peaks_empty_after_mz_range_removal": [msp("D_HORS_PLAGE", peaks=[(20.0101, 10), (30.0202, 30), (40.0303, 100)])],
    "insufficient_high_peaks": [msp("D_UN_SEUL_PIC_INTENSE", peaks=[(56.0495, 1), (69.0447, 1), (83.0604, 1),
                                                                    (110.0713, 2), (138.0662, 100)])],
    "entropy_score_lower_than_threshold": [msp("D_ENTROPIE", peaks=[(56.0495, 6), (110.0713, 6), (138.0662, 100)])],
    "precursor_mz_matches_no_adduct": [msp("D_MZ_SANS_ADDUIT", PrecursorMZ="500.1234")],
    "low_resolution_ms2": [msp("D_BASSE_RESOLUTION", peaks=[(56.0, 20), (69.0, 15), (83.1, 10), (110.1, 60),
                                                            (138.1, 100)])],
    "ms2_chemical_crash_test": [msp("D_FRAGMENTS_IMPOSSIBLES", peaks=[(56.0495, 20), (69.0447, 15), (83.0604, 10),
                                                                      (111.5555, 60), (139.4444, 100)])],
    # Structure illisible et InChIKey absente de la base : rien pour la recalculer
    "deleted_no_inchi_smiles_inchikey_after_re_calculation": [msp("D_STRUCTURE_ILLISIBLE", SMILES="C1CC(",
                                                                  InChIKey="ZZZZZZZZZZZZZZ-UHFFFAOYSA-N")],
}


def spectrum_id(text):
    return text.split("DB#: ")[1].split("\n")[0]


@pytest.fixture(scope="module")
def deletions_run(engine, tmp_path_factory):
    folder = tmp_path_factory.mktemp("suppressions")
    text = msp("GARDE") + "".join(s for group in DELETIONS.values() for s in group)
    return run_ok(engine, folder / "out", [write(folder, "cas.msp", text)])


@pytest.mark.parametrize("deletion_file, sid", [(f, spectrum_id(s)) for f, group in DELETIONS.items() for s in group])
def test_motif_de_suppression(deletions_run, deletion_file, sid):
    assert sid not in deletions_run.kept
    assert deletions_run.deletion_file(sid) == deletion_file
    assert deletions_run.deleted[sid][0]["DELETION_REASON"].startswith("spectrum deleted because")


def test_spectre_correct_garde_et_compte_des_suppressions(deletions_run):
    assert sorted(deletions_run.kept) == ["GARDE"]
    # Aucun fichier de suppression inattendu (« other_deletions.csv » : motif sans fichier prévu)
    assert sorted({r[0]["FILE"] for r in deletions_run.deleted.values()}) == sorted(DELETIONS)
    # Compteurs du nettoyage envoyés à l'interface et au rapport (spectrum_cleaning.rs)
    summary = next(str(m[0]) for m in deletions_run.messages if "No peaks list" in str(m[0]))
    counts = {k.strip(): int(v) for k, v in (line.split(":") for line in summary.strip().splitlines())}
    assert counts == {
        "No peaks list": 1,
        "No smiles, no inchi, no inchikey": 2,      # sans structure + « suspect »
        "No precursor mz": 4,
        "No or bad adduct": 4,                      # sans adduit, 2 adduits du mauvais mode, mode inconnu
        "Low entropy score": 1,
        "Minimum peaks not required": 1,
        "All peaks above precursor mz": 1,
        "No peaks in mz range": 1,
        "Minimum high peaks not required": 1,
    }
    # Total de fin de run : tous les spectres supprimés, y compris ceux de l'étape « Mass & Adduct Correction »
    # (basse résolution, contrôle chimique des fragments), qui n'étaient pas comptés
    total = sum(len(group) for group in DELETIONS.values())
    assert sum(len(rows) for rows in deletions_run.deleted.values()) == total
    assert deletions_run.said("Total deletions: %d" % total)


def test_doublons_le_plus_complet_garde_et_complete(engine, tmp_path):
    """Même spectre dans deux fichiers : celui qui a le plus de métadonnées est gardé, ses champs vides sont
    complétés par l'autre, qui est écrit dans duplicatas_removed.csv. Les noms de bases viennent de l'interface."""
    one = write(tmp_path, "une.msp", msp("DUP_COURT", peaks=C["peaks"], RetentionTime="2.5 min"))
    two = write(tmp_path, "deux.msp", msp("DUP_LONG", peaks=C["peaks"], Collision_energy="35 eV",
                                          Comments="commentaire assez long pour que ce spectre soit le plus complet"))
    result = run_ok(engine, tmp_path / "out", [one, two], input_db_names={one: "BaseUne", two: "BaseDeux"})
    assert sorted(result.kept) == ["DUP_LONG"]
    kept = result.kept["DUP_LONG"]
    assert (kept["DATABASE_NAME"], kept["FILENAME"], kept["COLLISIONENERGY"]) == ("BaseDeux", "deux.msp", "35 eV")
    assert kept["RT"] == "2.5"                                   # repris du doublon supprimé
    assert result.deletion_file("DUP_COURT") == "duplicatas_removed"
    assert result.deleted["DUP_COURT"][0]["DATABASE_NAME"] == "BaseUne"
    assert result.said("duplicatas removed: 1")


# ---------------------------------------------------------------------------------------------------------------
# Sorties : les douze bibliothèques
# ---------------------------------------------------------------------------------------------------------------
def test_douze_bibliotheques(engine, tmp_path):
    """POS / NEG × LC / GC / DI × expérimental / in silico : chaque spectre dans sa bibliothèque, dans les trois
    formats de sortie."""
    ei_pos, ei_neg = [(55, 20), (67, 15), (82, 30), (109, 60), (194, 100)], [(69, 20), (137, 15), (153, 30), (273, 60), (302, 100)]
    gc = dict(Instrument_type="GC-EI-TOF", Precursor_type=None, PrecursorMZ=None)   # GC : ni adduit ni précurseur
    di = dict(Instrument_type="DI-ESI-QTOF")
    predicted = dict(Comments="in-silico spectrum")
    Q = QUERCETIN
    spectra = [
        msp("POS_LC"), msp("POS_LC_In_Silico", **predicted),
        msp("POS_GC", peaks=ei_pos, **gc), msp("POS_GC_In_Silico", peaks=ei_pos[:-1] + [(194, 90)], **gc, **predicted),
        msp("POS_DI", **di), msp("POS_DI_In_Silico", **di, **predicted),
        msp("NEG_LC", mol=Q), msp("NEG_LC_In_Silico", mol=Q, **predicted),
        msp("NEG_GC", mol=Q, peaks=ei_neg, **gc), msp("NEG_GC_In_Silico", mol=Q, peaks=ei_neg[:-1] + [(302, 90)], **gc, **predicted),
        msp("NEG_DI", mol=Q, **di), msp("NEG_DI_In_Silico", mol=Q, **di, **predicted),
    ]
    result = run_ok(engine, tmp_path / "out", [write(tmp_path, "tout.msp", "".join(spectra))])
    libraries = sorted(spectrum_id(s) for s in spectra)
    assert {sid: row["LIB"] for sid, row in result.kept.items()} == {lib: lib for lib in libraries}
    assert result.deleted == {}
    for sid, row in result.kept.items():
        assert row["PREDICTED"] == ("true" if sid.endswith("In_Silico") else "false"), sid
        assert row["IONMODE"] == ("positive" if sid.startswith("POS") else "negative"), sid
    # Un fichier par bibliothèque dans chaque format ; chacun contient son spectre
    for lib in libraries:
        mode = lib[:3]
        with open(os.path.join(result.out, "JSON", mode, lib + ".json"), encoding="utf-8") as f:
            assert [s["SPECTRUMID"] for s in json.load(f)] == [lib]
        with open(os.path.join(result.out, "MSP", mode, lib.replace("_In_Silico", "_insilico") + ".msp"), encoding="utf-8") as f:
            text = f.read()
        assert text.count("NAME: ") == 1 and "SPECTRUMID=%s;" % lib in text
    assert len(result.files()) == 36
    # Spectre GC expérimental : pas d'adduit ni de m/z de précurseur, et ce n'est pas un motif de suppression
    assert (result.kept["POS_GC"]["PRECURSORTYPE"], result.kept["POS_GC"]["PRECURSORMZ"]) == ("NOT FOUND", "NOT FOUND")
    assert result.kept["POS_GC"]["PEAKS_LIST"] == "55 0.2;67 0.15;82 0.3;109 0.6;194 1"


def test_sorties_choisies(engine, tmp_path):
    """Options csv / msp / json : seuls les formats demandés sont écrits."""
    source = write(tmp_path, "a.msp", msp("A") + msp("B", mol=QUERCETIN))
    only_json = run_ok(engine, tmp_path / "json", [source], csv=0.0, msp=0.0)
    assert only_json.files() == ["JSON/NEG/NEG_LC.json", "JSON/POS/POS_LC.json"]
    only_csv = run_ok(engine, tmp_path / "csv", [source], json=0.0, msp=0.0)
    assert only_csv.files() == ["CSV/NEG/NEG_LC.csv", "CSV/POS/POS_LC.csv"]
    only_msp = run_ok(engine, tmp_path / "msp", [source], json=0.0, csv=0.0)
    assert only_msp.files() == ["MSP/NEG/NEG_LC.msp", "MSP/POS/POS_LC.msp"]


def test_sortie_mzspeclib(engine, tmp_path):
    """Sortie mzSpecLib : le moteur envoie les spectres à l'objet Python qui écrit les fichiers, comme dans
    FragHub.py. Le contenu des fichiers est vérifié dans test_mzspeclib_converter.py."""
    converter = pytest.importorskip("mzspeclib_converter")
    source = write(tmp_path, "a.msp", msp("A") + msp("B", mol=QUERCETIN))
    sink = converter.MzSpecLibSink()
    try:
        result = run(engine, tmp_path / "out", [source], mzspeclib_sink=sink, mzspeclib_json=1.0, csv=0.0, msp=0.0, json=0.0)
    finally:
        sink.close(terminate=True)
    assert result.error is None
    assert result.files() == ["mzSpecLib/NEG/NEG_LC.mzSpecLib.json", "mzSpecLib/POS/POS_LC.mzSpecLib.json"]
    with open(os.path.join(result.out, "mzSpecLib", "POS", "POS_LC.mzSpecLib.json"), encoding="utf-8") as f:
        library = json.load(f)
    assert len(library["spectra"]) == 1
    # Option demandée sans objet pour écrire (cas du moteur appelé seul) : pas de sortie mzSpecLib, pas d'erreur
    alone = run_ok(engine, tmp_path / "seul", [source], mzspeclib_json=1.0, csv=0.0, msp=0.0, json=0.0)
    assert alone.files() == []


# ---------------------------------------------------------------------------------------------------------------
# Structure, base interne, m/z du précurseur
# ---------------------------------------------------------------------------------------------------------------
def test_completion_par_la_base_interne(engine, tmp_path):
    spectra = (msp("DANS_LA_BASE", Name="nom du fichier")
               + msp("HORS_BASE", mol=PARACETAMOL, InChIKey=None)                 # InChIKey calculée par RDKit
               + msp("SMILES_ILLISIBLE", SMILES="C1CC("))                         # InChIKey dans la base
    result = run_ok(engine, tmp_path / "out", [write(tmp_path, "a.msp", spectra)])
    assert sorted(result.kept) == ["DANS_LA_BASE", "HORS_BASE", "SMILES_ILLISIBLE"]

    known = result.kept["DANS_LA_BASE"]
    assert known["NAME"] == "Caffeine (base)"                    # le nom de la base remplace celui du fichier
    assert [known[c] for c in CLASS_COLUMNS] == [BASE_ROWS[0][c] for c in CLASS_COLUMNS]
    assert (known["FORMULA"], known["INCHI"], known["INCHIKEY"]) == ("C8H10N4O2", CAFFEINE["inchi"], CAFFEINE["inchikey"])
    assert known["SMILES"] == "Cn1c(=O)c2c(ncn2C)n(C)c1=O"       # SMILES canonique de RDKit
    assert (known["EXACTMASS"], known["AVERAGEMASS"]) == ("194.08037556", "194.194")   # 8 décimales au plus

    unknown = result.kept["HORS_BASE"]
    assert unknown["NAME"] == "Paracetamol"                      # absent de la base : nom du fichier gardé
    assert [unknown[c] for c in CLASS_COLUMNS] == ["NOT FOUND"] * 6
    assert (unknown["INCHIKEY"], unknown["FORMULA"]) == (PARACETAMOL["inchikey"], "C8H9NO2")
    assert unknown["INCHI"] == PARACETAMOL["inchi"]

    # Structure du fichier illisible : SMILES de la base, retrouvé par l'InChIKey (rdkit_bridge.rs, deuxième passe)
    repaired = result.kept["SMILES_ILLISIBLE"]
    assert (repaired["SMILES"], repaired["INCHI"]) == ("Cn1c(=O)c2c(ncn2C)n(C)c1=O", CAFFEINE["inchi"])


def test_mz_du_precurseur(engine, tmp_path):
    spectra = (msp("MZ_ABSENT", PrecursorMZ=None)                # recalculé : masse exacte + adduit
               + msp("MZ_VIRGULE", PrecursorMZ="195,0877")       # virgule décimale
               + msp("MZ_D_UN_AUTRE_ADDUIT", PrecursorMZ="217.0696")   # m/z de [M+Na]+, adduit annoncé [M+H]+
               + msp("DEUX_FOIS_CHARGE", Precursor_type="[M+2H]2+", PrecursorMZ="98.0475",
                     peaks=[(56.0495, 20), (69.0447, 15), (83.0604, 10), (110.0713, 60), (138.0662, 100)]))
    result = run_ok(engine, tmp_path / "out", [write(tmp_path, "a.msp", spectra)])
    assert result.kept["MZ_ABSENT"]["PRECURSORMZ"] == "195.08765156"      # 8 décimales au plus (était …155999998)
    assert result.kept["MZ_VIRGULE"]["PRECURSORMZ"] == "195.0877"
    corrected = result.kept["MZ_D_UN_AUTRE_ADDUIT"]
    assert (corrected["PRECURSORTYPE"], corrected["PRECURSORMZ"]) == ("[M+Na]+", "217.0696")
    # Ion deux fois chargé : les fragments au-dessus du m/z du précurseur (jusqu'à sa masse) sont gardés
    assert result.kept["DEUX_FOIS_CHARGE"]["NUM PEAKS"] == "5"


# ---------------------------------------------------------------------------------------------------------------
# Filtres des pics : chacun peut être désactivé
# ---------------------------------------------------------------------------------------------------------------
FILTER_CASES = (msp("ENTROPIE", peaks=[(56.0495, 6), (110.0713, 6), (138.0662, 100)])
                + msp("DEUX_PICS", peaks=C["peaks"][-2:])
                + msp("UN_SEUL_PIC_INTENSE", peaks=[(56.0495, 1), (69.0447, 1), (83.0604, 1), (110.0713, 2), (138.0662, 100)])
                + msp("PIC_AU_DESSUS", peaks=C["peaks"] + [(250.1234, 50)])
                + msp("PIC_HORS_PLAGE", peaks=[(41.0386, 30)] + C["peaks"]))


def test_filtres_actifs(engine, tmp_path):
    result = run_ok(engine, tmp_path / "out", [write(tmp_path, "a.msp", FILTER_CASES)])
    for sid, deletion_file in [("ENTROPIE", "entropy_score_lower_than_threshold"),
                               ("DEUX_PICS", "number_of_peaks_below_threshold"),
                               ("UN_SEUL_PIC_INTENSE", "insufficient_high_peaks")]:
        assert result.deletion_file(sid) == deletion_file
    # Pic au-dessus du précurseur et pic sous 50 m/z retirés, le reste gardé et ramené à un maximum de 1.
    # Les deux spectres ont alors les mêmes pics : un seul est gardé, l'autre est un doublon (voir le test suivant).
    row = one_kept_others_duplicates(result, ["PIC_AU_DESSUS", "PIC_HORS_PLAGE"])
    assert row["PEAKS_LIST"] == "56.0495 0.2;69.0447 0.15;83.0604 0.1;110.0713 0.6;138.0662 1"


def test_splash_des_pics_ecrits(engine, tmp_path):
    """Le SPLASH écrit est celui des pics écrits, après les filtres. Avant, c'était celui des pics du fichier : il ne
    correspondait plus au spectre écrit dès qu'un filtre retirait un pic, et deux spectres devenus identiques après
    filtrage n'étaient pas dédoublonnés."""
    clean = write(tmp_path, "propre.msp", msp("PROPRE", peaks=C["peaks"]))
    extra = write(tmp_path, "filtre.msp", msp("AVEC_PIC_EN_TROP", peaks=C["peaks"] + [(250.1234, 50)]))
    reference = run_ok(engine, tmp_path / "ref", [clean]).kept["PROPRE"]
    out = tmp_path / "out"
    filtered = run_ok(engine, out, [extra]).kept["AVEC_PIC_EN_TROP"]
    assert filtered["PEAKS_LIST"] == reference["PEAKS_LIST"]             # le pic à 250 est retiré
    assert filtered["SPLASH"] == reference["SPLASH"]                     # donc même SPLASH que le spectre propre

    # Les mises à jour reconnaissent toujours le spectre à ses pics D'ORIGINE (updates.json garde leur SPLASH) :
    # relancé sur le même dossier, il n'est pas retraité
    with open(out / "updates.json", encoding="utf-8") as f:
        assert list(json.load(f)["SPLASH_LIST"]) != [filtered["SPLASH"]]
    again = run_ok(engine, out, [extra])
    assert again.said("previously cleaned: 1") and sorted(again.kept) == ["AVEC_PIC_EN_TROP"]

    # Dans le même run : les deux spectres sont des doublons
    both = run_ok(engine, tmp_path / "deux", [clean, extra])
    one_kept_others_duplicates(both, ["PROPRE", "AVEC_PIC_EN_TROP"])
    # Spectre supprimé au nettoyage : son SPLASH (celui des pics de son fichier) reste écrit dans DELETED_SPECTRUMS
    deleted = run_ok(engine, tmp_path / "supprime", [write(tmp_path, "d.msp", msp("DEUX", peaks=C["peaks"][-2:]))])
    assert deleted.deleted["DEUX"][0]["SPLASH"].startswith("splash10-")


def test_filtres_desactives(engine, tmp_path):
    off = dict(remove_spectrum_under_entropy_score=0.0, check_minimum_peak_requiered=0.0, keep_mz_in_range=0.0,
               remove_peak_above_precursormz=0.0, check_minimum_of_high_peaks_requiered=0.0, normalize_intensity=0.0)
    result = run_ok(engine, tmp_path / "out", [write(tmp_path, "a.msp", FILTER_CASES)], **off)
    assert result.deleted == {}
    assert {sid: row["PEAKS_LIST"] for sid, row in result.kept.items()} == {
        "ENTROPIE": "56.0495 6;110.0713 6;138.0662 100",
        "DEUX_PICS": "110.0713 60;138.0662 100",
        "UN_SEUL_PIC_INTENSE": "56.0495 1;69.0447 1;83.0604 1;110.0713 2;138.0662 100",
        "PIC_AU_DESSUS": "56.0495 20;69.0447 15;83.0604 10;110.0713 60;138.0662 100;250.1234 50",
        "PIC_HORS_PLAGE": "41.0386 30;56.0495 20;69.0447 15;83.0604 10;110.0713 60;138.0662 100"}


def test_seuils_des_filtres(engine, tmp_path):
    source = write(tmp_path, "a.msp", msp("S", peaks=C["peaks"]))
    # Trois pics au plus : les trois plus intenses, dans l'ordre des m/z
    reduced = run_ok(engine, tmp_path / "reduit", [source], reduce_peak_list_max_peaks=3.0).kept["S"]
    assert (reduced["PEAKS_LIST"], reduced["NUM PEAKS"]) == ("56.0495 0.2;110.0713 0.6;138.0662 1", "3")
    # Plage de m/z choisie
    ranged = run_ok(engine, tmp_path / "plage", [source], keep_mz_in_range_from_mz=60.0, keep_mz_in_range_to_mz=120.0).kept["S"]
    # 56 et 138 retirés, le plus intense vaut 1 ; 10/60 écrit avec 8 décimales (était 0.16666666666666666)
    assert ranged["PEAKS_LIST"] == "69.0447 0.25;83.0604 0.16666667;110.0713 1"
    # Seuils plus sévères : six pics exigés, puis entropie minimale de 0.99
    assert run_ok(engine, tmp_path / "six", [source], check_minimum_peak_requiered_n_peaks=6.0).deletion_file("S") \
        == "number_of_peaks_below_threshold"
    assert run_ok(engine, tmp_path / "entropie", [source], remove_spectrum_under_entropy_score_value=0.99).deletion_file("S") \
        == "entropy_score_lower_than_threshold"


def test_calcul_de_novo(engine, tmp_path):
    """Option de novo : chaque pic expliqué reçoit sa formule et son écart en ppm, dans les trois formats."""
    source = write(tmp_path, "a.msp", msp("N", peaks=C["peaks"]))
    result = run_ok(engine, tmp_path / "out", [source], calculate_de_novo=1.0)
    assert result.kept["N"]["PEAKS_LIST"] == ("56.0495 0.2 C3H6N+/9.52;69.0447 0.15 C3H5N2+/8.42;"
                                              "83.0604 0.1 C4H7N2+/6.38;110.0713 0.6 C5H8N3+/4.79;138.0662 1 C6H8N3O+/3.92")
    with open(os.path.join(result.out, "MSP", "POS", "POS_LC.msp"), encoding="utf-8") as f:
        assert "\n138.0662 1 C6H8N3O+/3.92\n" in f.read()
    # Sans l'option : pas d'annotation
    plain = run_ok(engine, tmp_path / "sans", [source])
    assert plain.kept["N"]["PEAKS_LIST"] == "56.0495 0.2;69.0447 0.15;83.0604 0.1;110.0713 0.6;138.0662 1"


# ---------------------------------------------------------------------------------------------------------------
# Mise à jour d'un dossier de sortie, remise à zéro, arrêt
# ---------------------------------------------------------------------------------------------------------------
def test_mise_a_jour_puis_remise_a_zero(engine, tmp_path):
    out = tmp_path / "out"
    # A3 : un pic au-dessus du précurseur sera retiré, son SPLASH écrit n'est donc pas celui de ses pics d'origine
    first = write(tmp_path, "a.msp", msp("A1") + msp("A2", mol=QUERCETIN)
                  + msp("A3", peaks=own_peaks(CAFFEINE, "A3") + [(250.1234, 50)]))
    second = write(tmp_path, "b.msp", msp("B1", mol=PARACETAMOL))

    run1 = run_ok(engine, out, [first])
    assert sorted(run1.kept) == ["A1", "A2", "A3"] and run1.said("previously cleaned: 0")
    libraries = {f: h for f, h in run1.snapshot().items() if f.split(os.sep)[0] in ("CSV", "MSP", "JSON")}

    # Même entrée relancée : rien de nouveau, les bibliothèques ne changent pas d'un octet
    run2 = run_ok(engine, out, [first])
    assert run2.said("previously cleaned: 3")
    assert {f: h for f, h in run2.snapshot().items() if f in libraries} == libraries
    assert sorted(run2.deleted) == ["A1", "A2", "A3"]
    assert {r["FILE"] for rows in run2.deleted.values() for r in rows} == {"previously_cleaned"}

    # Un fichier en plus : seul le nouveau spectre est traité, et ajouté aux bibliothèques existantes
    run3 = run_ok(engine, out, [first, second])
    assert run3.said("previously cleaned: 3")
    assert sorted(run3.kept) == ["A1", "A2", "A3", "B1"]        # (kept vérifie qu'aucun n'est écrit deux fois)
    assert [run3.kept[s]["LIB"] for s in ("A1", "A2", "A3", "B1")] == ["POS_LC", "NEG_LC", "POS_LC", "POS_LC"]
    with open(out / "JSON" / "POS" / "POS_LC.json", encoding="utf-8") as f:
        assert [s["SPECTRUMID"] for s in json.load(f)] == ["A1", "A3", "B1"]
    with open(out / "MSP" / "POS" / "POS_LC.msp", encoding="utf-8") as f:
        assert f.read().count("NAME: ") == 3
    with open(out / "updates.json", encoding="utf-8") as f:
        assert len(json.load(f)["SPLASH_LIST"]) == 4

    # Remise à zéro : le dossier ne contient plus que le résultat de ce run
    run4 = run_ok(engine, out, [second], reset_updates=1.0)
    assert run4.said("previously cleaned: 0")
    assert sorted(run4.kept) == ["B1"]
    assert run4.files() == ["CSV/POS/POS_LC.csv", "JSON/POS/POS_LC.json", "MSP/POS/POS_LC.msp"]
    assert len([f for f in os.listdir(out) if f.startswith("report_")]) == 1


def stop_after(n):
    """Indicateur d'arrêt comme celui de l'interface : faux n fois, puis vrai (le bouton Stop a été pressé)."""
    calls = [0]

    def flag():
        calls[0] += 1
        return calls[0] > n
    return flag


def test_arret_dossier_de_sortie_remis_comme_avant(engine, tmp_path):
    out = tmp_path / "out"
    first = write(tmp_path, "a.msp", msp("A1") + msp("A2", mol=QUERCETIN) + msp("A_SANS_PICS", peaks=[]))
    second = write(tmp_path, "b.msp", msp("B1", mol=PARACETAMOL))
    before = run_ok(engine, out, [first]).snapshot()
    assert before

    # Arrêt au milieu de la mise à jour (après le nettoyage et RDKit, avant l'écriture des bibliothèques)
    stopped = run(engine, out, [first, second], stop_flag=stop_after(6))
    assert str(stopped.error) == "Process stopped by user."
    assert stopped.snapshot() == before                          # ni fichier en plus, ni fichier modifié

    # Le run suivant fonctionne et traite le nouveau spectre
    after = run_ok(engine, out, [first, second])
    assert sorted(after.kept) == ["A1", "A2", "B1"]


def test_arret_pendant_le_premier_run_ne_laisse_rien(engine, tmp_path):
    source = write(tmp_path, "a.msp", msp("A1") + msp("A_SANS_PICS", peaks=[]))
    for n in (0, 2, 4, 6, 8):
        out = tmp_path / ("out_%d" % n)
        stopped = run(engine, out, [source], stop_flag=stop_after(n))
        assert str(stopped.error) == "Process stopped by user.", n
        assert stopped.snapshot() == {}, n
    # Sans arrêt, le même indicateur est consulté jusqu'au bout et le run réussit
    assert sorted(run_ok(engine, tmp_path / "out_fin", [source], stop_flag=stop_after(10 ** 6)).kept) == ["A1"]


def test_rien_a_traiter(engine, tmp_path):
    # Fichier vide, ou d'un type que FragHub ne lit pas : message, pas d'erreur, aucune sortie
    for name in ("vide.msp", "notes.txt"):
        nothing = run_ok(engine, tmp_path / name.replace(".", "_"), [write(tmp_path, name, "")])
        assert nothing.said("No files to process") and nothing.files() == []
    # Tous les spectres supprimés au nettoyage : le motif est écrit, aucune bibliothèque
    all_deleted = run_ok(engine, tmp_path / "out", [write(tmp_path, "a.msp", msp("SANS_PICS", peaks=[]))])
    assert all_deleted.said("No spectrums to process after cleaning")
    assert all_deleted.files() == ["DELETED_SPECTRUMS/peaks_list_is_empty.csv"]
