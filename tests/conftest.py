# -*- coding: utf-8 -*-
"""Réglages communs à tous les tests Python (pytest lit ce fichier en premier).

- Les modules de scripts/ et de scripts/build_dataset/ s'importent directement (`import internal_database`),
  comme quand FragHub les lance (réglé dans pyproject.toml, [tool.pytest.ini_options] pythonpath).
- `tmp_path` (fourni par pytest) donne à chaque test un dossier temporaire neuf, effacé ensuite : un test ne touche
  jamais au dépôt ni aux données réelles.
- Windows : le moteur Rust (fraghub_rust) a besoin des DLL de RDKit C++. Depuis Python 3.8, le PATH ne sert plus à
  trouver les DLL d'un module compilé : le dossier est ajouté explicitement, comme le fait FragHub.py.
"""
import http.server
import os
import sys
import threading

import pytest

if sys.platform == "win32":
    _rdkit_bin = os.path.join(os.environ.get("RDKIT_PREFIX") or os.path.join(os.path.expanduser("~"), "rdkit_cpp"),
                              "Library", "bin")
    if os.path.isdir(_rdkit_bin):
        os.add_dll_directory(_rdkit_bin)

def pytest_addoption(parser):
    parser.addoption("--update-reference", action="store_true", default=False,
                     help="test de bout en bout : remplace la sortie de référence par celle du run (changement voulu)")


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
