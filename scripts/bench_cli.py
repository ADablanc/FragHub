"""Lanceur en ligne de commande de l'ancien FragHub Python (branche benchmark, copie de develop).

L'ancienne version n'a pas de CLI : elle ne tourne que derrière le serveur FastAPI piloté par l'interface.
Ce script fait exactement ce que fait le serveur (chargement des bases internes, puis MAIN avec les
mêmes paramètres), sans serveur ni interface, et note pour chaque étape la durée, la mémoire, le nombre
d'éléments et les cœurs CPU utilisés, avec le même module perf_log.py que la version Rust.
Les deux journaux ont donc exactement les mêmes colonnes et se comparent ligne à ligne.

Utilisation (depuis la racine du dépôt) :
    python scripts\\bench_cli.py --input_directory <fichier(s)> --output_directory <dossier>

Les options et leurs valeurs par défaut sont celles de la CLI Rust (FragHub.py --cli).
Le journal est écrit dans <dossier de sortie>\\PERF_LOGS (perf_<date>.csv / .txt et historique.csv).
"""
import argparse
import os
import sys
import time
import traceback

# Racine du dépôt dans le chemin d'import, pour que « import scripts.… » fonctionne
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

# Sortie console en UTF-8 même redirigée vers un fichier (évite les erreurs « charmap » sous Windows)
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

from scripts import perf_log  # noqa: E402
from scripts.backend_vars import parameters_dict  # noqa: E402
import scripts.globals_vars as g_vars  # noqa: E402


def _yes(v):
    return 1.0 if v == "yes" else 0.0


def parse_args():
    p = argparse.ArgumentParser(description="FragHub Python (ancienne version) — lancement en ligne de commande avec mesures")
    p.add_argument("--input_directory", nargs="+", required=True, help="Fichiers d'entrée (.json, .msp, .mgf, .csv)")
    p.add_argument("--output_directory", type=str, required=True, help="Dossier de sortie")
    p.add_argument("--quiet", action="store_true", help="N'affiche que les étapes, pas chaque tâche")
    # Mêmes options et mêmes valeurs par défaut que la CLI Rust
    for name, default in [("normalize_intensity", "yes"), ("remove_peak_above_precursormz", "yes"),
                          ("check_minimum_peak_requiered", "yes"), ("reduce_peak_list", "yes"),
                          ("remove_spectrum_under_entropy_score", "yes"), ("keep_mz_in_range", "yes"),
                          ("check_minimum_of_high_peaks_requiered", "yes"), ("calculate_de_novo", "no"),
                          ("csv", "yes"), ("msp", "yes"), ("json", "yes"), ("reset_updates", "no")]:
        p.add_argument(f"--{name}", type=str, choices=["yes", "no"], default=default)
    for name, default in [("check_minimum_peak_requiered_n_peaks", 3.0), ("reduce_peak_list_max_peaks", 500.0),
                          ("remove_spectrum_under_entropy_score_value", 0.5), ("keep_mz_in_range_from_mz", 50.0),
                          ("keep_mz_in_range_to_mz", 2000.0),
                          ("check_minimum_of_high_peaks_requiered_intensity_percent", 5.0),
                          ("check_minimum_of_high_peaks_requiered_no_peaks", 2.0), ("de_novo_ppm_tolerance", 10.0)]:
        p.add_argument(f"--{name}", type=float, default=default)
    return p.parse_args()


def build_params(a):
    # Mêmes clés que celles que l'interface envoie au serveur (/run-analysis)
    params = {
        "input_directory": [os.path.abspath(f) for f in a.input_directory],
        "output_directory": os.path.abspath(a.output_directory),
    }
    for name in ["normalize_intensity", "remove_peak_above_precursormz", "check_minimum_peak_requiered",
                 "reduce_peak_list", "remove_spectrum_under_entropy_score", "keep_mz_in_range",
                 "check_minimum_of_high_peaks_requiered", "calculate_de_novo", "csv", "msp", "json",
                 "reset_updates"]:
        params[name] = _yes(getattr(a, name))
    for name in ["check_minimum_peak_requiered_n_peaks", "reduce_peak_list_max_peaks",
                 "remove_spectrum_under_entropy_score_value", "keep_mz_in_range_from_mz", "keep_mz_in_range_to_mz",
                 "check_minimum_of_high_peaks_requiered_intensity_percent",
                 "check_minimum_of_high_peaks_requiered_no_peaks", "de_novo_ppm_tolerance"]:
        params[name] = float(getattr(a, name))
    return params


def main():
    a = parse_args()
    params = build_params(a)
    for f in params["input_directory"]:
        if not os.path.isfile(f):
            print(f"Fichier introuvable : {f}")
            return 1

    # Chargement des bases internes (ce que fait /init-data au lancement de l'application)
    print("Chargement des bases internes…", flush=True)
    t0 = time.perf_counter()
    g_vars.load_internal_databases()
    init_s = time.perf_counter() - t0
    perf_log.record_init(init_s)
    print(f"Bases internes chargées en {init_s:.1f} s", flush=True)

    parameters_dict.clear()
    parameters_dict.update(params)

    # Import après le chargement : MAIN importe des modules qui lisent globals_vars
    from scripts.MAIN import MAIN

    # Les modules appellent parfois les callbacks avec deux arguments (ex. total_items_callback(n, 0)) :
    # on ne garde que le premier, comme le faisait le serveur.
    def step_cb(*args):
        if args:
            perf_log.on_step(args[0])
            print(f"\n{str(args[0]).strip()}", flush=True)

    def total_cb(*args):
        if args:
            perf_log.on_total(args[0])

    def prefix_cb(*args):
        if args:
            perf_log.on_prefix(args[0])
            if not a.quiet:
                print(f"   {str(args[0]).strip()}", flush=True)

    def item_type_cb(*args):
        if args:
            perf_log.on_item_type(args[0])

    def progress_cb(*args):
        if args:
            perf_log.on_progress(args[0])

    def deletion_cb(*args):
        if args:
            perf_log.on_deletion(args[0])
            print(str(args[0]).strip(), flush=True)

    def completion_cb(*args):
        if args:
            perf_log.on_completion(args[0])
            print(str(args[0]).strip(), flush=True)

    perf_log.start(params)
    status = None
    try:
        MAIN(progress_callback=progress_cb, total_items_callback=total_cb, prefix_callback=prefix_cb,
             item_type_callback=item_type_cb, step_callback=step_cb, completion_callback=completion_cb,
             deletion_callback=deletion_cb, stop_flag=lambda: False)
    except KeyboardInterrupt:
        status = "arrêté (Ctrl+C)"
    except BaseException as e:  # MemoryError comprise : on écrit quand même le journal
        traceback.print_exc()
        status = f"erreur : {type(e).__name__}: {e}"
    finally:
        perf_log.finish(status)
    return 0 if status is None else 1


if __name__ == "__main__":
    sys.exit(main())
