"""Écriture des fichiers mzSpecLib (format PSI, https://psidev.info/mzspeclib) de FragHub.

Le moteur Rust envoie les spectres de chaque bibliothèque (POS_LC, NEG_LC...) par paquets à un objet
`MzSpecLibSink` (writers/mzspeclib_sender.rs) : chaque spectre arrive sous la forme du texte JSON d'un spectre
FragHub (mêmes noms de champs et mêmes valeurs que la sortie JSON). Ici :
  - chaque champ FragHub est traduit en terme du vocabulaire PSI-MS (`to_spectrum`) ; un champ sans terme
    adapté devient un attribut libre (MS:1003275 / MS:1003276) qui garde son nom FragHub ; un champ vide ou
    « NOT FOUND » n'est pas écrit ;
  - l'objet est mis en forme par la bibliothèque `mzspeclib` (JSONSpectralLibraryWriter), en parallèle sur
    tous les cœurs ; les paquets sont recollés dans l'ordre d'arrivée ;
  - le fichier obtenu est exactement celui qu'écrirait `mzspeclib` pour toute la bibliothèque d'un coup
    (en-tête de bibliothèque compris), sans avoir tous les spectres en mémoire, à une correction près : une valeur
    libre qui contient « | » reste entière (voir `_keep_whole_values`).
Chaque terme a été vérifié avec le validateur de mzspeclib (vocabulaire PSI-MS 4.1.258).
"""
import collections
import json
import multiprocessing
import os
import re
import warnings

from mzspeclib.analyte import Analyte
from mzspeclib.backends.json import JSONSpectralLibraryWriter
from mzspeclib.backends.memory import InMemorySpectrumLibrary
from mzspeclib.spectrum import Spectrum


def _guard_output_file(path, append):
    """Protection du dossier de sortie (output_guard.rs) : un fichier mzSpecLib remplacé ou complété est remis comme
    avant si le run s'arrête ensuite. Importé ici, dans le processus principal seulement (les processus de
    conversion n'ont pas à charger le moteur Rust). Absent (tests sans le moteur) : rien n'est noté."""
    try:
        from fraghub_rust import guard_output_file
    except ImportError:  # pragma: no cover
        return
    guard_output_file(path, append)
from mzspeclib.annotation import parse_annotation  # mzPAF (réexporté par mzspeclib)

# ---------------------------------------------------------------------------------------------
# Correspondance FragHub -> PSI-MS
# ---------------------------------------------------------------------------------------------

# Valeur absente : vide, « NOT FOUND » (normalize_to_not_found.rs), « NaN » (sortie JSON)
EMPTY = re.compile(r"^\s*(|nan|not found)\s*$", re.I)

UNIT = "UO:0000000|unit"
EV = "UO:0000266|electronvolt"
PERCENT = "UO:0000187|percent"
MINUTE = "UO:0000031|minute"
DALTON = "UO:0000221|dalton"
MZ = "MS:1000040|m/z"
OTHER_NAME = "MS:1003275|other attribute name"
OTHER_VALUE = "MS:1003276|other attribute value"

# Méthodes de fragmentation écrites par FragHub (normalize_collision_energy.rs)
DISSOCIATION = {
    "CID": "MS:1000133|collision-induced dissociation",
    "CAD": "MS:1000133|collision-induced dissociation",
    "HCD": "MS:1000422|beam-type collision-induced dissociation",
    "ETD": "MS:1000598|electron transfer dissociation",
    "ECD": "MS:1000250|electron capture dissociation",
    "EAD": "MS:1003294|electron activated dissociation",
    "UVPD": "MS:1003246|ultraviolet photodissociation",
    "IRMPD": "MS:1000262|infrared multiphoton dissociation",
    "PQD": "MS:1000599|pulsed q dissociation",
    "EThcD": "MS:1002631|electron-transfer/higher-energy collision dissociation",
    "ETciD": "MS:1003182|electron-transfer/collision-induced dissociation",
    "in-source CID": "MS:1001880|in-source collision-induced dissociation",
    "LOW-ENERGY CID": "MS:1000433|low-energy collision-induced dissociation",
    "low-energy CID": "MS:1000433|low-energy collision-induced dissociation",
}

# Ionisations écrites par FragHub (normalize_ionization.rs)
IONIZATION = {
    "ESI": "MS:1000073|electrospray ionization",
    "APCI": "MS:1000070|atmospheric pressure chemical ionization",
    "APPI": "MS:1000382|atmospheric pressure photoionization",
    "EI": "MS:1000389|electron ionization",
    "CI": "MS:1000071|chemical ionization",
    "MALDI": "MS:1000075|matrix-assisted laser desorption ionization",
    "FAB": "MS:1000074|fast atom bombardment ionization",
}

POLARITY = {"positive": "MS:1000130|positive scan", "negative": "MS:1000129|negative scan"}

# Champs sans terme adapté dans le vocabulaire : attributs libres, dans cet ordre
# (FILENAME, SPECTRUMID : les termes « source data file » et « external reference identifier » exigent un terme
#  comme valeur ; FILEHASH : SHA-256 de la taille du fichier, pas de son contenu ; RESOLUTION : catégorie
#  HIGH / LOW, pas une mesure ; INSTRUMENT, INSTRUMENTTYPE : termes du vocabulaire à trouver avec l'arbre des
#  instruments)
FREE_FIELDS = [
    "SPECTRUMID", "FILENAME", "DATABASE_NAME", "FILEHASH", "RESOLUTION", "SYNON", "INSTRUMENTTYPE", "INSTRUMENT",
    "ENTROPY", "CLASSYFIRE_SUPERCLASS", "CLASSYFIRE_CLASS", "CLASSYFIRE_SUBCLASS",
    "NPCLASS_PATHWAY", "NPCLASS_SUPERCLASS", "NPCLASS_CLASS",
]

_NUM = r"\d+(?:\.\d+)?"
_ENERGY_SINGLE = re.compile(rf"({_NUM}) (eV|NCE)")
_ENERGY_RAMP = re.compile(rf"Ramp ({_NUM})/({_NUM}) (eV|NCE)")
# Charge écrite à la fin de l'adduit normalisé : "[M-H]-" -> -1, "[M+2H]2+" -> 2, "[M]+" -> 1,
# "[M]+*" (ion radical, GC-EI) -> 1
_ADDUCT_CHARGE = re.compile(r"\](\d*)([+-])\*?$")
# Formule de novo d'un pic : formule de l'ion, signe (toujours « + » aujourd'hui), « / », écart en ppm
_DENOVO = re.compile(r"([A-Z][A-Za-z0-9]*)[+-]?/([+-]?\d+(?:\.\d+)?)")
# Accession d'un terme de vocabulaire (« MS:1000133 », « UO:0000266 »)
_CURIE = re.compile(r"[A-Za-z]+:\d+")


def _value(d, key):
    """Valeur d'un champ FragHub, sous forme de texte ; None si elle est vide."""
    v = d.get(key)
    if v is None:
        return None
    s = str(v).strip()
    return None if EMPTY.match(s) else s


def _float(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def _add_free(obj, name, value):
    """Attribut libre : nom FragHub + valeur, dans un même groupe."""
    group = obj.get_next_group_identifier()
    obj.add_attribute(OTHER_NAME, name, group)
    obj.add_attribute(OTHER_VALUE, value, group)


def _add_with_unit(obj, term, value, unit):
    group = obj.get_next_group_identifier()
    obj.add_attribute(term, value, group)
    obj.add_attribute(UNIT, unit, group)


def _add_energy(spectrum, text):
    """Énergie de collision écrite par FragHub (normalize_collision_energy.rs). Le vocabulaire n'accepte que
    l'électronvolt (collision energy) et le pourcentage (normalized collision energy) : toute autre écriture
    (V, kV, sans unité, paliers, négative, CE0...) reste un attribut libre, telle quelle."""
    m = _ENERGY_SINGLE.fullmatch(text)
    if m:
        if m.group(2) == "eV":
            _add_with_unit(spectrum, "MS:1000045|collision energy", float(m.group(1)), EV)
        else:
            _add_with_unit(spectrum, "MS:1000138|normalized collision energy", float(m.group(1)), PERCENT)
        return
    m = _ENERGY_RAMP.fullmatch(text)
    if m:
        if m.group(3) == "eV":
            _add_with_unit(spectrum, "MS:1002013|collision energy ramp start", float(m.group(1)), EV)
            _add_with_unit(spectrum, "MS:1002014|collision energy ramp end", float(m.group(2)), EV)
        else:
            _add_with_unit(spectrum, "MS:1002218|percent collision energy ramp start", float(m.group(1)), PERCENT)
            _add_with_unit(spectrum, "MS:1002219|percent collision energy ramp end", float(m.group(2)), PERCENT)
        return
    _add_free(spectrum, "COLLISIONENERGY", text)


def adduct_charge(adduct):
    """Charge lue à la fin de l'adduit normalisé par FragHub ; None si elle n'y est pas écrite."""
    m = _ADDUCT_CHARGE.search(adduct)
    if not m:
        return None
    n = int(m.group(1)) if m.group(1) else 1
    return n if m.group(2) == "+" else -n


def denovo_annotation(text):
    """Formule de novo d'un pic (de_novo_calculation.rs : « C2H3O+/1.23 » = formule de l'ion / écart en ppm)
    -> annotation mzPAF « f{C2H3O}/1.23ppm ». Charge 1 sous-entendue ; en mzPAF la charge est écrite sans signe,
    le signe vient de la polarité du spectre (scan polarity). Écriture non reconnue : pas d'annotation."""
    m = _DENOVO.fullmatch(str(text).strip())
    if not m:
        return []
    return parse_annotation(f"f{{{m.group(1)}}}/{m.group(2)}ppm")


def to_spectrum(d, key):
    """Spectre FragHub (dict, noms et valeurs de la sortie JSON) -> objet mzSpecLib."""
    sp = Spectrum()
    sp.key = key
    v = _value(d, "NAME")
    if v:
        sp.add_attribute("MS:1003061|library spectrum name", v)
    v = _value(d, "SPLASH")
    if v:
        sp.add_attribute("MS:1002599|splash key", v)
    v = _value(d, "PREDICTED")
    if v:
        if v.lower() == "true":
            sp.add_attribute("MS:1003072|spectrum origin type", "MS:1003074|predicted spectrum")
        elif v.lower() == "false":
            sp.add_attribute("MS:1003072|spectrum origin type", "MS:1003073|observed spectrum")
        else:
            _add_free(sp, "PREDICTED", v)
    v = _value(d, "MSLEVEL")
    if v:
        if v.isdigit():
            sp.add_attribute("MS:1000511|ms level", int(v))
        else:
            _add_free(sp, "MSLEVEL", v)  # ex. "2-3"
    v = _value(d, "PRECURSORMZ")
    if v:
        x = _float(v)
        if x is not None:
            _add_with_unit(sp, "MS:1003208|experimental precursor monoisotopic m/z", x, MZ)
        else:
            _add_free(sp, "PRECURSORMZ", v)
    v = _value(d, "IONMODE")
    if v:
        if v.lower() in POLARITY:
            sp.add_attribute("MS:1000465|scan polarity", POLARITY[v.lower()])
        else:
            _add_free(sp, "IONMODE", v)
    v = _value(d, "IONIZATION")
    if v:
        if v in IONIZATION:
            sp.add_attribute("MS:1000008|ionization type", IONIZATION[v])
        else:
            _add_free(sp, "IONIZATION", v)
    v = _value(d, "FRAGMENTATIONMETHODE")
    if v:
        if v in DISSOCIATION:
            sp.add_attribute("MS:1000044|dissociation method", DISSOCIATION[v])
        else:
            _add_free(sp, "FRAGMENTATIONMETHODE", v)
    v = _value(d, "COLLISIONENERGY")
    if v:
        _add_energy(sp, v)
    v = _value(d, "RT")
    if v:
        x = _float(v)
        if x is not None:
            _add_with_unit(sp, "MS:1000894|retention time", x, MINUTE)  # FragHub écrit le RT en minutes
        else:
            _add_free(sp, "RT", v)
    v = _value(d, "COMMENT")
    if v:
        sp.add_attribute("MS:1003168|library spectrum comment", v)
    for field in FREE_FIELDS:
        v = _value(d, field)
        if v:
            _add_free(sp, field, v)

    analyte = Analyte("1")
    for field, term in (("FORMULA", "MS:1000866|molecular formula"), ("SMILES", "MS:1000868|SMILES string"),
                        ("INCHI", "MS:1003403|InChI"), ("INCHIKEY", "MS:1002894|InChIKey")):
        v = _value(d, field)
        if v:
            analyte.add_attribute(term, v)
    v = _value(d, "PRECURSORTYPE")
    if v:
        analyte.add_attribute("MS:1002813|adduct ion formula", v)
        charge = adduct_charge(v)
        if charge is not None:
            analyte.add_attribute("MS:1000041|charge state", charge)
    for field, term in (("EXACTMASS", "MS:1003637|theoretical neutral monoisotopic mass"),
                        ("AVERAGEMASS", "MS:1003638|theoretical neutral average mass")):
        v = _value(d, field)
        if v:
            x = _float(v)
            if x is not None:
                _add_with_unit(analyte, term, x, DALTON)
            else:
                _add_free(analyte, field, v)
    sp.add_analyte(analyte)

    # Pics : m/z, intensité, et formule de novo éventuelle (3e valeur) en annotation mzPAF
    peaks = d.get("PEAKS_LIST") or []
    sp.peak_list = [[float(p[0]), float(p[1]), denovo_annotation(p[2]) if len(p) > 2 else [], []] for p in peaks]
    sp.add_attribute("MS:1003059|number of peaks", len(peaks))
    return sp


# ---------------------------------------------------------------------------------------------
# Mise en forme : exactement celle de JSONSpectralLibraryWriter (json.dump(buffer, indent=2, sort_keys=True))
# ---------------------------------------------------------------------------------------------

_SPECTRA_EMPTY = '"spectra": []'
_ITEM_SEP = ",\n    "  # entre deux spectres du tableau "spectra" (niveau 2)


def library_text_parts(name, version):
    """En-tête et fin du fichier, autour des spectres : (avant le premier spectre, après le dernier)."""
    library = InMemorySpectrumLibrary(create_index=False)
    library.add_attribute("MS:1003186|library format version", "1.0")
    library.add_attribute("MS:1003188|library name", name)
    library.add_attribute("MS:1003189|library description", f"Spectral library built by FragHub {version}")
    writer = JSONSpectralLibraryWriter(_NullHandle())
    writer.write_header(library)
    text = json.dumps(writer.buffer, indent=2, sort_keys=True)
    i = text.index(_SPECTRA_EMPTY)
    head = text[:i] + '"spectra": [\n    '
    tail = "\n  ]" + text[i + len(_SPECTRA_EMPTY):]
    return head, tail, text


class _NullHandle:
    closed = False

    def write(self, *_):
        pass

    def close(self):
        self.closed = True


def _keep_whole_values(attributes):
    """Le writer de mzspeclib coupe toute valeur texte au premier « | » (« accession|nom » d'un terme) : une valeur
    libre comme « Polyketides|Terpenoids » (NPClassifier) devenait l'accession « Polyketides » et la valeur
    « Terpenoids ». La valeur est remise entière quand la partie gauche n'est pas une accession de terme."""
    for a in attributes:
        acc = a.get("value_accession")
        if acc is not None and not _CURIE.fullmatch(str(acc)):
            a["value"] = f"{acc}|{a['value']}"
            del a["value_accession"]


def spectrum_text(spectrum, writer):
    """Texte JSON d'un spectre, déjà indenté pour le tableau "spectra"."""
    writer.write_spectrum(spectrum)
    payload = writer.buffer["spectra"].pop()
    _keep_whole_values(payload["attributes"])
    for analyte in payload["analytes"].values():
        _keep_whole_values(analyte["attributes"])
    return json.dumps(payload, indent=2, sort_keys=True).replace("\n", "\n    ")


# --- Travail fait dans les processus parallèles --------------------------------------------------

_worker_writer = None


def _worker_init():
    global _worker_writer
    warnings.simplefilter("ignore")
    # Ctrl+C est géré par le processus principal (arrêt propre, src/stop.rs) : les processus de conversion
    # l'ignorent, sinon ils s'arrêtaient en plein travail et bloquaient la conversion
    try:
        import signal
        signal.signal(signal.SIGINT, signal.SIG_IGN)
    except (ImportError, ValueError, OSError):  # pragma: no cover
        pass
    _worker_writer = JSONSpectralLibraryWriter(_NullHandle())


def convert_batch(args):
    """Convertit un paquet : (texte des spectres, nombre écrit, erreurs). Un spectre qui ne peut pas être
    converti est signalé et laissé de côté ; les autres gardent leur numéro (clé)."""
    first_key, texts = args
    writer = _worker_writer if _worker_writer is not None else JSONSpectralLibraryWriter(_NullHandle())
    out, errors = [], []
    for i, text in enumerate(texts):
        key = first_key + i
        try:
            out.append(spectrum_text(to_spectrum(json.loads(text), key), writer))
        except Exception as e:  # noqa: BLE001 - un spectre fautif ne doit pas arrêter la bibliothèque
            writer.buffer["spectra"].clear()
            errors.append(f"spectre {key} : {e}")
    return _ITEM_SEP.join(out), len(out), errors


# ---------------------------------------------------------------------------------------------
# Réception des spectres envoyés par Rust
# ---------------------------------------------------------------------------------------------

_LAST_KEY = re.compile(rb'"name": "library spectrum key",\s*"value": (\d+)')


def _last_key(path, size, tail_len):
    """Clé du dernier spectre d'un fichier mzSpecLib déjà écrit (cherchée depuis la fin)."""
    with open(path, "rb") as f:
        window = 1 << 20
        while True:
            start = max(0, size - tail_len - window)
            f.seek(start)
            data = f.read(size - tail_len - start)
            found = list(_LAST_KEY.finditer(data))
            if found:
                return int(found[-1].group(1))
            if start == 0:
                return None
            window *= 4


class MzSpecLibSink:
    """Reçoit les spectres d'une bibliothèque après l'autre : begin(), add() par paquet, end().
    Les paquets sont convertis en parallèle ; au plus `max_pending` paquets attendent à la fois.
    Le fichier est écrit à côté (« .part ») puis mis en place à la fin : un arrêt en cours de route ne laisse
    ni fichier incomplet ni fichier précédent abîmé."""

    def __init__(self, workers=None, progress_cb=None):
        self.workers = workers or max(1, multiprocessing.cpu_count() - 1)
        self.max_pending = 2 * self.workers
        self.pool = None
        self._reset()

    def _reset(self):
        self.path = None
        self.part = None
        self.pending = collections.deque()
        self.next_key = 1
        self.written = 0
        self.skipped = 0
        self.append_mode = False

    def _ensure_pool(self):
        if self.pool is None:
            try:
                self.pool = multiprocessing.Pool(processes=self.workers, initializer=_worker_init)
            except Exception as e:  # noqa: BLE001
                print(f"[mzSpecLib] Conversion parallèle impossible ({e}) : conversion dans ce processus.")
                self.pool = False
                _worker_init()

    def begin(self, path, name, update=False, version=""):
        self._ensure_pool()
        self._reset()
        self.path = path
        os.makedirs(os.path.dirname(path), exist_ok=True)
        self.head, self.tail, self.empty_text = library_text_parts(name, version)
        size = os.path.getsize(path) if os.path.exists(path) else 0
        if update and size > len(self.tail):
            with open(path, "rb") as f:
                f.seek(size - len(self.tail))
                same_tail = f.read() == self.tail.encode("ascii")
            last = _last_key(path, size, len(self.tail)) if same_tail else None
            if last is not None:
                # Mise à jour : les nouveaux spectres sont ajoutés à la suite, avec les clés suivantes
                self.append_mode = True
                self.next_key = last + 1
            elif size == len(self.empty_text) and open(path, "rb").read() == self.empty_text.encode("ascii"):
                pass  # bibliothèque existante sans spectre : réécrite, rien n'est perdu
            else:
                print(f"[mzSpecLib] {os.path.basename(path)} : fichier existant d'un autre format, réécrit "
                      f"avec les seuls nouveaux spectres (relancer avec la remise à zéro des mises à jour pour "
                      f"tout réécrire).")
        self.part = open(path + ".part", "w", encoding="ascii", newline="\n")
        self.first = True

    def add(self, texts):
        job = (self.next_key, list(texts))
        self.next_key += len(job[1])
        if self.pool:
            self.pending.append(self.pool.apply_async(convert_batch, (job,)))
            while len(self.pending) > self.max_pending:
                self._write(self.pending.popleft().get())
        else:
            self._write(convert_batch(job))

    def _write(self, result):
        text, n, errors = result
        for e in errors:
            print(f"[mzSpecLib] Spectre ignoré dans {os.path.basename(self.path)} ({e})")
        self.skipped += len(errors)
        if n:
            if not self.first or self.append_mode:
                self.part.write(_ITEM_SEP)
            self.part.write(text)
            self.first = False
            self.written += n

    def end(self):
        while self.pending:
            self._write(self.pending.popleft().get())
        self.part.close()
        part_path = self.path + ".part"
        if self.append_mode:
            # Fin du fichier existant retirée, nouveaux spectres ajoutés, fin remise
            if self.written:
                _guard_output_file(self.path, True)
                with open(self.path, "r+b") as out:
                    out.seek(0, os.SEEK_END)
                    out.truncate(out.tell() - len(self.tail))
                    out.seek(0, os.SEEK_END)
                    with open(part_path, "rb") as src:
                        while block := src.read(1 << 22):
                            out.write(block)
                    out.write(self.tail.encode("ascii"))
            os.remove(part_path)
        else:
            final_tmp = self.path + ".tmp"
            with open(final_tmp, "w", encoding="ascii", newline="\n") as out:
                if self.written:
                    out.write(self.head)
                    with open(part_path, "r", encoding="ascii", newline="\n") as src:
                        while block := src.read(1 << 22):
                            out.write(block)
                    out.write(self.tail)
                else:
                    out.write(self.empty_text)
            os.remove(part_path)
            _guard_output_file(self.path, False)
            os.replace(final_tmp, self.path)
        if self.skipped:
            print(f"[mzSpecLib] {self.skipped} spectre(s) ignoré(s) dans {os.path.basename(self.path)}.")
        self._reset()

    def abort(self):
        """Arrêt demandé : rien n'est écrit, le fichier précédent (mise à jour) reste tel quel."""
        self.pending.clear()
        if self.part is not None and not self.part.closed:
            self.part.close()
        for p in (self.path and self.path + ".part", self.path and self.path + ".tmp"):
            if p and os.path.exists(p):
                try:
                    os.remove(p)
                except OSError:
                    pass
        self.close(terminate=True)
        self._reset()

    def close(self, terminate=False):
        if self.pool:
            if terminate:
                self.pool.terminate()
            else:
                self.pool.close()
            self.pool.join()
        self.pool = None
