# -*- coding: utf-8 -*-
"""
Écrit THIRD_PARTY_LICENSES.txt : les logiciels d'autres auteurs livrés avec FragHub, leur licence et le texte de cette
licence (avec les mentions de copyright). Les licences MIT, BSD, Apache... demandent que ces textes accompagnent
toute distribution en binaire : le fichier est copié dans les installateurs (GUI/package.json, extraResources).

Ce qui est livré, et donc listé :
- les crates Rust compilées dans le moteur (cargo metadata, dépendances normales du moteur, toutes plateformes) ;
- les paquets Python embarqués par PyInstaller (dépendances d'exécution de FragHub dans uv.lock) et Python lui-même ;
- les bibliothèques de RDKit C++ (environnement conda RDKIT_PREFIX, sinon ~/rdkit_cpp : paquets qui fournissent
  des bibliothèques partagées) ;
- les paquets JavaScript compilés dans l'interface (liste NPM_BUNDLED, ci-dessous) et Electron.

Usage (à la racine du dépôt, avec le .venv de FragHub) :
    python scripts/third_party_licenses.py [--output THIRD_PARTY_LICENSES.txt]
Une partie introuvable (cargo absent, node_modules manquant...) est signalée dans le fichier et à l'écran, le reste
est quand même écrit. Bibliothèque standard de Python uniquement (Python 3.11 ou plus, pour tomllib).
"""
import argparse
import collections
import datetime
import glob
import hashlib
import importlib.metadata
import json
import os
import re
import subprocess
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUST_DIR = os.path.join(ROOT, "scripts", "fraghub_rust")
GUI_DIR = os.path.join(ROOT, "GUI")

# Paquets npm dont le code est compilé dans les pages de l'interface (GUI/.output/public). Liste relevée le
# 2026-11-13 dans les sourcemaps d'un `nuxt generate` (sourcemap: { client: true }) : à refaire si les dépendances de
# l'interface changent. Les outils de construction (Vite, Nuxt CLI...) ne sont pas livrés.
NPM_BUNDLED = ["@socket.io/component-emitter", "@unhead/vue", "@vue/reactivity", "@vue/runtime-core", "@vue/runtime-dom",
               "@vue/shared", "darkreader", "defu", "destr", "devalue", "engine.io-client", "engine.io-parser", "h3",
               "hookable", "nuxt", "ofetch", "socket.io-client", "socket.io-parser", "ufo", "unctx", "vue", "vuetify",
               "vuetify-nuxt-module", "electron"]

# Textes types, pour un paquet qui ne fournit pas son propre fichier de licence (le copyright est celui de ses auteurs)
MIT_TEXT = """MIT License

Copyright (c) {authors}

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."""
BSD3_TEXT = """BSD 3-Clause License

Copyright (c) {authors}

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
   products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."""

LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense)([-_.].*)?$", re.I)
SKIP_PYTHON = {"fraghub", "fraghub-rust"}


def warn(problems, message):
    problems.append(message)
    print("  attention : " + message)


def license_files(folder, max_depth=1):
    """Fichiers de licence d'un dossier (et de ses sous-dossiers licenses/, LICENSES/ s'il y en a)."""
    out = []
    if not os.path.isdir(folder):
        return out
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if os.path.isfile(path) and LICENSE_FILE.match(name):
            out.append(path)
        elif os.path.isdir(path) and name.lower() in ("licenses", "license") and max_depth:
            out += [os.path.join(path, n) for n in sorted(os.listdir(path)) if os.path.isfile(os.path.join(path, n))]
    return out


def read_text(path):
    with open(path, "rb") as f:
        return f.read().decode("utf-8", "replace").replace("\r\n", "\n").strip()


# --- Rust ---

def rust_components(problems):
    try:
        out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=RUST_DIR,
                             capture_output=True, text=True, encoding="utf-8", check=True).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        warn(problems, "crates Rust non listées (cargo metadata a échoué : %s)" % e)
        return []
    meta = json.loads(out)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        node = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        for dep in nodes[node]["deps"]:
            # dépendances normales et de construction (build.rs) : pas celles des tests
            if any(k["kind"] in (None, "build") for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)
    comps = []
    for pid in seen:
        p = packages[pid]
        folder = os.path.dirname(p["manifest_path"])
        files = license_files(folder)
        if p.get("license_file"):
            extra = os.path.join(folder, p["license_file"])
            if os.path.isfile(extra) and extra not in files:
                files.append(extra)
        comps.append({"group": "Rust crate", "name": p["name"], "version": p["version"],
                      "license": p.get("license") or ("see " + p["license_file"] if p.get("license_file") else "?"),
                      "url": p.get("repository") or p.get("homepage") or "https://crates.io/crates/" + p["name"],
                      "authors": ", ".join(p.get("authors") or []), "files": files})
    return comps


# --- Python ---

def python_components(problems):
    try:
        with open(os.path.join(ROOT, "uv.lock"), "rb") as f:
            lock = tomllib.load(f)
    except OSError as e:
        warn(problems, "paquets Python non listés (uv.lock illisible : %s)" % e)
        return []
    packages = {p["name"]: p for p in lock["package"]}
    run, stack = set(), [d["name"] for d in packages["fraghub"].get("dependencies", [])]
    while stack:
        name = stack.pop()
        if name in run or name not in packages:
            continue
        run.add(name)
        stack += [d["name"] for d in packages[name].get("dependencies", [])]
    comps = []
    for name in sorted(run - SKIP_PYTHON):
        try:
            dist = importlib.metadata.distribution(name)
        except importlib.metadata.PackageNotFoundError:
            warn(problems, "paquet Python %s absent de l'environnement : texte de licence non repris" % name)
            comps.append({"group": "Python package", "name": name, "version": packages[name].get("version", "?"),
                          "license": "?", "url": "https://pypi.org/project/" + name, "authors": "", "files": []})
            continue
        md = dist.metadata
        lic = md.get("License-Expression") or ""
        if not lic:
            classifiers = [c.split("::")[-1].strip() for c in md.get_all("Classifier") or [] if c.startswith("License ::")]
            lic = "; ".join(classifiers) or (md.get("License") or "?").split("\n")[0][:80]
        files = []
        for f in dist.files or []:
            parts = str(f).replace("\\", "/").split("/")
            if parts[0].endswith(".dist-info") and (LICENSE_FILE.match(parts[-1]) or (len(parts) > 2 and parts[1] == "licenses")):
                files.append(str(dist.locate_file(f)))
        urls = [u.split(",", 1)[-1].strip() for u in md.get_all("Project-URL") or []]
        comps.append({"group": "Python package", "name": md["Name"], "version": md["Version"], "license": lic,
                      "url": md.get("Home-page") or (urls[0] if urls else "https://pypi.org/project/" + name),
                      "authors": md.get("Author") or md.get("Author-email") or "", "files": sorted(set(files))})
    # Python lui-même (python312.dll / libpython, et sa bibliothèque standard)
    candidates = [os.path.join(sys.base_prefix, "LICENSE.txt"), os.path.join(sys.base_prefix, "LICENSE"),
                  os.path.join(sys.base_prefix, "lib", "python%d.%d" % sys.version_info[:2], "LICENSE.txt")]
    found = [p for p in candidates if os.path.isfile(p)][:1]
    if not found:
        warn(problems, "licence de Python introuvable dans %s" % sys.base_prefix)
    comps.append({"group": "Python", "name": "CPython", "version": "%d.%d.%d" % sys.version_info[:3],
                  "license": "PSF-2.0", "url": "https://www.python.org", "authors": "Python Software Foundation", "files": found})
    return comps


# --- RDKit C++ (conda) ---

def conda_components(problems):
    prefix = os.environ.get("RDKIT_PREFIX") or os.path.join(os.path.expanduser("~"), "rdkit_cpp")
    metas = glob.glob(os.path.join(prefix, "conda-meta", "*.json"))
    if not metas:
        warn(problems, "bibliothèques de RDKit C++ non listées (pas d'environnement conda dans %s)" % prefix)
        return []
    # Exécutable déjà construit (python build_backend.py) : seuls les paquets dont une bibliothèque y a été copiée
    # par PyInstaller sont livrés. Sinon, tous ceux qui fournissent une bibliothèque partagée (liste plus large).
    built = {os.path.basename(p).lower() for p in glob.glob(os.path.join(ROOT, "scripts", "dist", "FragHub_Backend", "**", "*"), recursive=True)}
    comps = []
    for path in sorted(metas):
        with open(path, encoding="utf-8") as f:
            m = json.load(f)
        shared = [x for x in m.get("files", []) if re.search(r"\.(dll|so(\.\d+)*|dylib)$", x, re.I)]
        if built:
            shared = [x for x in shared if os.path.basename(x).lower() in built]
        if not shared:
            continue   # en-têtes, outils, bibliothèques non copiées : rien de livré
        pkg_dir = m.get("extracted_package_dir") or ""
        files = license_files(os.path.join(pkg_dir, "info"), max_depth=1) if pkg_dir else []
        files = [p for p in files if os.path.isfile(p)]
        if not files:
            warn(problems, "texte de licence du paquet conda %s introuvable (cache des paquets vidé ?)" % m["name"])
        comps.append({"group": "C/C++ library (conda-forge)", "name": m["name"], "version": m.get("version", "?"),
                      "license": m.get("license") or "?", "url": "https://anaconda.org/conda-forge/" + m["name"],
                      "authors": "", "files": files})
    return comps


# --- JavaScript ---

def npm_components(problems):
    lock_path = os.path.join(GUI_DIR, "package-lock.json")
    try:
        with open(lock_path, encoding="utf-8") as f:
            lock = json.load(f)["packages"]
    except (OSError, ValueError, KeyError) as e:
        warn(problems, "paquets JavaScript non listés (%s : %s)" % (lock_path, e))
        return []
    comps = []
    for name in NPM_BUNDLED:
        entry = lock.get("node_modules/" + name)
        if entry is None:
            warn(problems, "paquet npm %s absent de package-lock.json (liste NPM_BUNDLED à revoir)" % name)
            continue
        folder = os.path.join(GUI_DIR, "node_modules", *name.split("/"))
        files = license_files(folder, max_depth=0)
        info = {}
        try:
            with open(os.path.join(folder, "package.json"), encoding="utf-8") as f:
                info = json.load(f)
        except (OSError, ValueError):
            warn(problems, "paquet npm %s non installé (npm install dans GUI/) : texte de licence non repris" % name)
        lic = entry.get("license") or info.get("license") or "?"
        if isinstance(lic, dict):
            lic = lic.get("type", "?")
        repo = info.get("repository")
        url = (repo.get("url") if isinstance(repo, dict) else repo) or info.get("homepage") or "https://www.npmjs.com/package/" + name
        author = info.get("author")
        comps.append({"group": "JavaScript package", "name": name, "version": entry.get("version", "?"), "license": lic,
                      "url": url, "authors": author.get("name", "") if isinstance(author, dict) else (author or ""),
                      "files": files})
    return comps


# --- Écriture ---

def main():
    ap = argparse.ArgumentParser(description=__doc__.strip().splitlines()[0])
    ap.add_argument("--output", default=os.path.join(ROOT, "THIRD_PARTY_LICENSES.txt"))
    args = ap.parse_args()
    problems = []
    comps = rust_components(problems) + python_components(problems) + conda_components(problems) + npm_components(problems)
    comps.sort(key=lambda c: (c["group"], c["name"].lower(), c["version"]))

    # Paquet sans fichier de licence : texte type de sa licence (une seule des licences au choix « A OR B » suffit).
    # Le texte Apache-2.0 est repris d'un autre paquet (il ne contient pas de copyright propre).
    apache = next((read_text(p) for c in comps for p in c["files"]
                   if re.search(r"apache", os.path.basename(p), re.I) and read_text(p).lstrip().startswith("Apache License")), None)
    for c in comps:
        if c["files"]:
            continue
        ids = re.findall(r"[A-Za-z0-9.+-]+", c["license"].replace("/", " OR "))
        authors = re.sub(r"\s*<[^>]*>", "", c["authors"]).strip() or "the %s authors" % c["name"]
        if "MIT" in ids:
            body = MIT_TEXT.format(authors=authors)
        elif "BSD-3-Clause" in ids:
            body = BSD3_TEXT.format(authors=authors)
        elif "Apache-2.0" in ids and apache:
            body = apache
        else:
            continue
        c["generated"] = body
    # Textes identiques (même fichier MIT ou Apache copié dans plusieurs paquets) : écrits une seule fois
    texts = collections.OrderedDict()
    for c in comps:
        c["text_ids"] = []
        for body in [read_text(p) for p in c["files"]] + ([c["generated"]] if c.get("generated") else []):
            if not body:
                continue
            key = hashlib.sha256(body.encode("utf-8")).hexdigest()[:12]
            texts.setdefault(key, {"body": body, "users": []})["users"].append("%s %s" % (c["name"], c["version"]))
            c["text_ids"].append(key)
        if not c["text_ids"]:
            c["text_ids"] = ["-"]

    version = "2.0.0"
    try:
        with open(os.path.join(ROOT, "pyproject.toml"), "rb") as f:
            version = tomllib.load(f)["project"]["version"]
    except (OSError, KeyError):
        pass
    lines = ["FragHub %s - third-party software" % version, "=" * 40, "",
             "FragHub is distributed under the CeCILL 2.1 license (see LICENSE). It includes the software below, made by",
             "other authors and distributed under their own licenses. For each one: version, license (SPDX), source, and",
             "the reference of its license text(s), given in full in the second part of this file.",
             "Electron also ships its own notices next to the application (LICENSE.electron.txt, LICENSES.chromium.html).",
             "", "Generated on %s by scripts/third_party_licenses.py." % datetime.date.today().isoformat(), ""]
    if problems:
        lines += ["Incomplete parts:"] + ["  - " + p for p in problems] + [""]
    group = None
    for c in comps:
        if c["group"] != group:
            group = c["group"]
            lines += ["", "%s (%d)" % (group, sum(1 for x in comps if x["group"] == group)), "-" * 40]
        lines.append("%s %s | %s | %s | text: %s" % (c["name"], c["version"], c["license"], c["url"], ", ".join(c["text_ids"])))
        if c["text_ids"] == ["-"]:
            lines.append("    (no license file shipped with this package: see its source above%s)"
                         % ("; authors: " + c["authors"] if c["authors"] else ""))
        elif c.get("generated"):
            lines.append("    (no license file shipped with this package: standard text of its license given)")
    lines += ["", "", "LICENSE TEXTS", "=" * 40]
    for key, t in texts.items():
        lines += ["", "-" * 80, "[%s] used by: %s" % (key, ", ".join(t["users"])), "-" * 80, "", t["body"]]
    with open(args.output, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print("Écrit : %s (%d logiciels, %d textes de licence%s)" % (os.path.relpath(args.output, ROOT), len(comps), len(texts),
          ", %d parties incomplètes" % len(problems) if problems else ""))


if __name__ == "__main__":
    main()
