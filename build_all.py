"""Reconstruit tout FragHub : back-end (Rust + PyInstaller), front-end (Nuxt), puis l'installeur Electron.

Usage :  python build_all.py                (tout)
         python build_all.py --skip-rust    (ne recompile pas le Rust)
         python build_all.py --skip-backend (ni Rust ni PyInstaller : front + installeur)
         python build_all.py --skip-frontend
         python build_all.py --no-installer (s'arrête avant electron-builder)
         python build_all.py --dry-run      (affiche les commandes sans rien lancer)

Résultat : GUI/dist_electron/ (.dmg sur macOS, .exe sur Windows)
"""
import sys

from build_backend import build_pyinstaller, build_rust, ensure_database
from build_frontend import build_frontend, npm
from build_utils import (BuildError, DRY_RUN, GUI_DIR, IS_WINDOWS, ROOT, SCRIPTS_DIR,
                         main_wrapper, run, step, venv_bin)

# Modèles NSIS d'electron-builder, modifiés avant chaque création de l'installeur (voir patch_nsis_templates)
NSIS_TEMPLATES = GUI_DIR / "node_modules" / "app-builder-lib" / "templates" / "nsis"
NSIS_PATCHES = [
    # Liste des fichiers installés affichée d'office (electron-builder la cachait, même sans bouton pour l'ouvrir)
    ("common.nsh",
     "ShowInstDetails nevershow\n",
     "ShowInstDetails show\n"),
    # La liste des fichiers installés reste affichée pendant l'installation (electron-builder la coupait)
    ("installSection.nsh",
     "${IfNot} ${Silent}\n  SetDetailsPrint none\n${endif}\n",
     "# FragHub : la liste des fichiers installés reste affichée (patch_nsis_templates dans build_all.py)\n"),
    # Fichiers de l'application copiés un par un par NSIS, au lieu d'une archive 7z décompressée par le module
    # Nsis7z : celui-ci remettait la barre de progression à zéro pour sa propre progression (la barre se
    # remplissait, revenait en arrière, puis se remplissait de nouveau) et n'affichait aucun nom de fichier.
    # La copie de l'installeur (1,2 Go) dans AppData est retirée : elle ne sert qu'aux mises à jour automatiques
    # d'electron-updater, que FragHub n'utilise pas.
    ("include/installer.nsh",
     "      !insertmacro extractEmbeddedAppPackage\n"
     "      # electron always uses per user app data\n"
     "      ${if} $installMode == \"all\"\n"
     "        SetShellVarContext current\n"
     "      ${endif}\n"
     "      !insertmacro copyFile \"$EXEPATH\" \"$LOCALAPPDATA\\${APP_INSTALLER_STORE_FILE}\"\n"
     "      ${if} $installMode == \"all\"\n"
     "        SetShellVarContext all\n"
     "      ${endif}\n",
     "      # FragHub : fichiers copiés un par un (patch_nsis_templates dans build_all.py, GUI/build/installer.nsh)\n"
     "      File /r \"${FRAGHUB_APP_DIR}\\*.*\"\n"),
]


def patch_nsis_templates(templates_dir=NSIS_TEMPLATES):
    """
    Modifie les modèles NSIS d'electron-builder (dans node_modules) pour un installeur Windows plus classique :
    une seule barre de progression, du début à la fin, et la liste des fichiers installés. electron-builder n'a pas
    d'option pour ça. Sans effet si c'est déjà fait ; erreur si un modèle a changé (nouvelle version
    d'electron-builder) : il faut alors revoir NSIS_PATCHES.
    """
    for name, old, new in NSIS_PATCHES:
        path = templates_dir / name
        with open(path, encoding="utf-8", newline="") as f:
            text = f.read()
        eol = "\r\n" if "\r\n" in text else "\n"
        old_text, new_text = old.replace("\n", eol), new.replace("\n", eol)
        if old_text not in text and new_text in text:
            continue   # déjà modifié par un build précédent
        if text.count(old_text) != 1:
            raise BuildError(f"Modèle NSIS d'electron-builder inattendu : {path}\n"
                             "electron-builder a sans doute changé de version : revoir NSIS_PATCHES dans build_all.py.")
        if DRY_RUN:
            print(f"  (dry-run) modification de {path}")
            continue
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(text.replace(old_text, new_text))


def build_installer():
    step("Installeur Electron (electron-builder)")
    backend = SCRIPTS_DIR / "dist" / "FragHub_Backend" / ("FragHub_Backend.exe" if IS_WINDOWS else "FragHub_Backend")
    if not backend.exists() and not DRY_RUN:
        raise BuildError(f"Back-end compilé introuvable ({backend}). Lancez d'abord python build_backend.py.")
    if not (GUI_DIR / ".output" / "public" / "index.html").exists() and not DRY_RUN:
        raise BuildError("Front-end compilé introuvable (GUI/.output/public). Lancez d'abord python build_frontend.py.")
    # Licences des logiciels livrés avec FragHub (copiées dans l'installeur : GUI/package.json, extraResources)
    run([str(venv_bin("python")), str(SCRIPTS_DIR / "third_party_licenses.py")], cwd=ROOT,
        title="Licences des logiciels tiers (THIRD_PARTY_LICENSES.txt)")
    patch_nsis_templates()
    run([npm(), "run", "build:electron"], cwd=GUI_DIR, title="Création de l'installeur dans GUI/dist_electron")


def build_all():
    """Reconstruit tout FragHub : back-end, front-end, installeur Electron.

Usage :  python build_all.py [--skip-rust] [--skip-backend] [--skip-frontend] [--no-installer] [--dry-run]"""
    args = sys.argv
    if "--skip-backend" not in args:
        ensure_database()
        if "--skip-rust" not in args:
            build_rust()
        build_pyinstaller()
    if "--skip-frontend" not in args:
        build_frontend()
    if "--no-installer" not in args:
        build_installer()


if __name__ == "__main__":
    sys.exit(main_wrapper(build_all))
