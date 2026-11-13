# -*- coding: utf-8 -*-
"""Réglages communs à tous les tests Python (pytest lit ce fichier en premier).

- Les modules de scripts/ et de scripts/build_dataset/ s'importent directement (`import internal_database`),
  comme quand FragHub les lance (réglé dans pyproject.toml, [tool.pytest.ini_options] pythonpath).
- `tmp_path` (fourni par pytest) donne à chaque test un dossier temporaire neuf, effacé ensuite : un test ne touche
  jamais au dépôt ni aux données réelles.
- Windows : le moteur Rust (fraghub_rust) a besoin des DLL de RDKit C++. Depuis Python 3.8, le PATH ne sert plus à
  trouver les DLL d'un module compilé : le dossier est ajouté explicitement, comme le fait FragHub.py.
- Les tests qui lancent le moteur (test_pipeline.py, test_bout_en_bout.py) utilisent le moteur COMPILÉ installé
  dans .venv, pas les sources Rust. Si une source Rust est plus récente que ce moteur, ces tests s'arrêtent en le
  disant (fixture `moteur_a_jour`) : sinon ils testeraient l'ancien code et diraient que tout va bien.
"""
import datetime
import glob
import http.server
import importlib.util
import os
import sys
import threading

import pytest

if sys.platform == "win32":
    _rdkit_bin = os.path.join(os.environ.get("RDKIT_PREFIX") or os.path.join(os.path.expanduser("~"), "rdkit_cpp"),
                              "Library", "bin")
    if os.path.isdir(_rdkit_bin):
        os.add_dll_directory(_rdkit_bin)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUST_DIR = os.path.join(ROOT, "scripts", "fraghub_rust")


def pytest_addoption(parser):
    parser.addoption("--update-reference", action="store_true", default=False,
                     help="test de bout en bout : remplace la sortie de référence par celle du run (changement voulu)")
    parser.addoption("--allow-stale-engine", action="store_true", default=False,
                     help="lance les tests du moteur même si une source Rust est plus récente que le moteur compilé")


def engine_file():
    """Fichier compilé du moteur Rust installé (fraghub_rust*.pyd sous Windows, .so ailleurs), ou None."""
    try:
        spec = importlib.util.find_spec("fraghub_rust")
    except (ImportError, ValueError):
        return None
    if spec is None or not spec.origin:
        return None
    if spec.origin.endswith((".pyd", ".so", ".dylib")):
        return spec.origin
    # Module installé sous forme de dossier (maturin develop) : le fichier compilé est à côté de __init__.py
    found = [f for ext in ("pyd", "so") for f in glob.glob(os.path.join(os.path.dirname(spec.origin), "fraghub_rust*." + ext))]
    return found[0] if found else None


def newest_rust_source():
    """(date de modification, chemin) de la source la plus récente du moteur : src/ (Rust et C++), build.rs, Cargo.toml."""
    files = [os.path.join(RUST_DIR, n) for n in ("build.rs", "Cargo.toml")]
    for root, _, names in os.walk(os.path.join(RUST_DIR, "src")):
        files += [os.path.join(root, n) for n in names]
    dated = [(os.path.getmtime(f), f) for f in files if os.path.isfile(f)]
    return max(dated) if dated else None


def engine_state():
    """(fichier du moteur, date de compilation, source plus récente ou None). (None, None, None) sans moteur."""
    engine = engine_file()
    if engine is None:
        return None, None, None
    built = os.path.getmtime(engine)
    newest = newest_rust_source()
    stale = newest[1] if newest and newest[0] > built else None
    return engine, built, stale


def pytest_report_header(config):
    engine, built, stale = engine_state()
    if engine is None:
        return "moteur Rust : non installé (les tests qui le lancent sont sautés)"
    line = "moteur Rust : compilé le %s" % datetime.datetime.fromtimestamp(built).strftime("%d/%m/%Y à %H:%M")
    if stale:
        line += " — PLUS ANCIEN que %s : recompiler (python build_backend.py --rust-only)" % os.path.relpath(stale, ROOT)
    return line


@pytest.fixture(scope="session")
def moteur_a_jour(request):
    """À demander par tout test qui lance le moteur compilé : échoue si une source Rust a été modifiée depuis la
    compilation. `--allow-stale-engine` passe outre (modification qui ne touche que des tests Rust, par exemple)."""
    engine, built, stale = engine_state()
    if stale and not request.config.getoption("--allow-stale-engine"):
        pytest.fail("Le moteur Rust installé (%s) est plus ancien que %s : ce test vérifierait l'ancien code.\n"
                    "Recompiler : python build_backend.py --rust-only\n"
                    "Ou, si seuls des tests Rust ont changé : ajouter --allow-stale-engine"
                    % (datetime.datetime.fromtimestamp(built).strftime("%d/%m/%Y %H:%M"), os.path.relpath(stale, ROOT)),
                    pytrace=False)


class _FileHandler(http.server.BaseHTTPRequestHandler):
    """Petit serveur HTTP local : sert un fichier, avec ou sans reprise (en-tête Range), comme Zenodo."""
    content = b""
    accept_range = True
    requests = []

    def do_GET(self):
        type(self).requests.append(self.headers.get("Range"))
        data, status = self.content, 200
        rng = self.headers.get("Range")
        if rng and self.accept_range:
            start = int(rng.split("=")[1].split("-")[0])
            data, status = self.content[start:], 206
        self.send_response(status)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):   # pas de journal du serveur dans la sortie des tests
        pass


@pytest.fixture
def file_server():
    """Serveur HTTP local qui sert `serve(contenu)` ; renvoie (serve, adresse, classe du gestionnaire)."""
    handler = type("Handler", (_FileHandler,), {"requests": []})
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    def serve(content, accept_range=True):
        handler.content = content
        handler.accept_range = accept_range

    yield serve, "http://127.0.0.1:%d/archive.zip" % server.server_address[1], handler
    server.shutdown()
    server.server_close()
