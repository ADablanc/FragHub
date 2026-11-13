"""Lance tous les tests de FragHub : moteur Rust (cargo test), Python (pytest), interface (vitest).

Usage :  python run_tests.py                  tout
         python run_tests.py --rust           seulement le moteur Rust
         python run_tests.py --python         seulement les tests Python (dossier tests/)
         python run_tests.py --gui            seulement l'interface (GUI/)
         python run_tests.py --rust adduit    un filtre : seulement les tests dont le nom contient « adduit »
         python run_tests.py --fast           sans le test de bout en bout (tout FragHub, une à deux minutes)
         python run_tests.py --dry-run        affiche les commandes sans les lancer

Le script règle lui-même ce que les tests demandent (rien à taper avant) :
  - Windows : DLL de RDKit C++ (~/rdkit_cpp/Library/bin ou RDKIT_PREFIX) et de Python (python3xx.dll) dans le PATH ;
  - macOS / Linux : édition de liens avec libpython (PyO3 ne la fait pas pour un module Python).
Voir TESTS.md pour le détail.
"""
import os
import subprocess
import sys
import time

from build_utils import (DRY_RUN, GUI_DIR, IS_WINDOWS, RDKIT_PREFIX, ROOT, RUST_DIR, BuildError, fail, format_duration,
                         ok, require_tool, step, venv_bin)

PARTS = ("--rust", "--python", "--gui")
FAST = "--fast" in sys.argv


def venv_python_info():
    """Dossier de base, dossier des bibliothèques et version du Python du .venv (celui que PyO3 utilise)."""
    code = ("import sys, sysconfig; print(sys.base_prefix); print(sysconfig.get_config_var('LIBDIR') or ''); "
            "print(sysconfig.get_config_var('LDVERSION') or sysconfig.get_python_version())")
    out = subprocess.run([str(venv_bin("python")), "-c", code], capture_output=True, text=True, check=True).stdout
    base, libdir, ldversion = (out.splitlines() + ["", "", ""])[:3]
    return base, libdir, ldversion


def test_env():
    """Variables d'environnement des tests (voir l'en-tête)."""
    env = os.environ.copy()
    env["RDKIT_PREFIX"] = str(RDKIT_PREFIX)
    if DRY_RUN or not venv_bin("python").exists():
        return env
    base, libdir, ldversion = venv_python_info()
    if IS_WINDOWS:
        rdkit_bin = RDKIT_PREFIX / "Library" / "bin"
        # RDKit C++ en premier : un autre RDKit plus ancien dans le PATH (environnement conda) a des DLL du même nom
        env["PATH"] = os.pathsep.join([str(rdkit_bin), base, env.get("PATH", "")])
    else:
        flags = f"-L {libdir} -l dylib=python{ldversion} -C link-arg=-Wl,-rpath,{libdir}"
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " " + flags).strip()
        lib = str(RDKIT_PREFIX / "lib")
        key = "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
        env[key] = os.pathsep.join(p for p in (lib, env.get(key, "")) if p)
    return env


def launch(cmd, cwd, env):
    cmd = [str(c) for c in cmd]
    print(f"  $ (cd {cwd}) {' '.join(cmd)}", flush=True)
    if DRY_RUN:
        return True
    return subprocess.run(cmd, cwd=cwd, env=env).returncode == 0


def rust(env, filters):
    step("Moteur Rust : cargo test")
    cargo = require_tool("cargo", "Installez Rust : https://rustup.rs")
    if not (RDKIT_PREFIX / ("Library" if IS_WINDOWS else "") / "include" / "rdkit").is_dir() and not DRY_RUN:
        raise BuildError(f"RDKit C++ introuvable dans {RDKIT_PREFIX} (voir TESTS.md, « Ce qu'il faut installer »).")
    return launch([cargo, "test", "--lib", *filters], RUST_DIR, env)


def python(env, filters):
    step("Python : pytest (dossier tests/)")
    py = venv_bin("python")
    if not py.exists() and not DRY_RUN:
        raise BuildError("Environnement .venv introuvable : lancez « uv sync » à la racine du projet.")
    cmd = [py, "-m", "pytest", "tests"]
    if filters:
        cmd += ["-k", " or ".join(filters)]
    if FAST:
        cmd += ["-m", "not bout_en_bout"]
    return launch(cmd, ROOT, env)


def gui(env, filters):
    step("Interface : vitest (GUI/)")
    import json
    with open(GUI_DIR / "package.json", encoding="utf-8") as f:
        scripts = json.load(f).get("scripts", {})
    if "test" not in scripts:
        print("  (pas encore de tests pour l'interface : rien à lancer)")
        return None
    npm = require_tool("npm.cmd" if IS_WINDOWS else "npm", "Installez Node.js : https://nodejs.org")
    # node_modules présent mais sans Vitest (installé avant l'ajout des tests) : même message
    vitest = GUI_DIR / "node_modules" / ".bin" / ("vitest.cmd" if IS_WINDOWS else "vitest")
    if not vitest.exists() and not DRY_RUN:
        raise BuildError("Outils de test de l'interface absents : lancez « npm install » dans GUI/.")
    cmd = [npm, "run", "test"]
    if filters:
        cmd += ["--", "-t", filters[0]]
    return launch(cmd, GUI_DIR, env)


def main():
    if "-h" in sys.argv or "--help" in sys.argv:
        print(__doc__.strip())
        return 0
    args = [a for a in sys.argv[1:] if a not in ("--dry-run", "--fast")]
    chosen = [a for a in args if a in PARTS] or list(PARTS)
    filters = [a for a in args if a not in PARTS]
    start = time.time()
    results = {}
    try:
        env = test_env()
        for part, func in (("--rust", rust), ("--python", python), ("--gui", gui)):
            if part in chosen:
                results[part[2:]] = func(env, filters)
    except BuildError as e:
        fail(str(e))
        return 1
    except KeyboardInterrupt:
        fail("Interrompu par l'utilisateur.")
        return 130
    print()
    for name, passed in results.items():
        if passed is None:
            print(f"  {name} : pas de tests")
        else:
            (ok if passed else fail)(f"{name} : {'tous les tests passent' if passed else 'au moins un test échoue (voir plus haut)'}")
    print(f"Durée : {format_duration(time.time() - start)}")
    return 0 if all(r is not False for r in results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
