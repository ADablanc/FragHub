import asyncio
import socketio
import uvicorn
import multiprocessing
import threading
import traceback
import mzspeclib_converter
import time
from contextlib import asynccontextmanager
from fastapi import FastAPI, BackgroundTasks, HTTPException
from fastapi.responses import FileResponse
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field, ConfigDict

import sys
import os

# Windows, mode développement : le moteur Rust utilise les DLL de RDKit C++ (installé avec mamba
# dans ~/rdkit_cpp ou dans le dossier indiqué par RDKIT_PREFIX). Depuis Python 3.8, le PATH n'est plus
# utilisé pour trouver les DLL d'un module compilé : il faut indiquer le dossier explicitement.
# (Dans l'exécutable PyInstaller, les DLL sont copiées à côté du module : rien à faire.)
if sys.platform == 'win32' and not getattr(sys, 'frozen', False):
    _rdkit_bin = os.path.join(os.environ.get('RDKIT_PREFIX') or os.path.join(os.path.expanduser('~'), 'rdkit_cpp'), 'Library', 'bin')
    if os.path.isdir(_rdkit_bin):
        os.add_dll_directory(_rdkit_bin)

import fraghub_rust
import perf_log  # PERF_LOG : journal de performances temporaire (voir perf_log.py)
import internal_database  # base interne de molécules : téléchargée depuis Zenodo si elle n'est pas sur le disque

parameters_dict = {}

if getattr(sys, 'frozen', False):
    BASE_DIR = sys._MEIPASS
else:
    BASE_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))


is_cli_mode = "--cli" in sys.argv
is_quiet_mode = "--quiet" in sys.argv
# Sous Windows (et dans l'exécutable), chaque processus de la conversion mzSpecLib relit ce fichier
# au démarrage : la bannière et la redirection du journal ne doivent se faire que dans le processus
# principal (sinon la bannière est répétée et chaque processus vide fraghub_debug.txt).
is_main_process = multiprocessing.current_process().name == "MainProcess"

# Redirection forcée de la sortie standard et d'erreur vers un fichier (UNIQUEMENT si packagé avec PyInstaller)
if getattr(sys, 'frozen', False) and not is_cli_mode and is_main_process:
    log_path = os.path.join(os.path.expanduser("~"), "fraghub_debug.txt")
    sys.stdout = open(log_path, 'w')
    sys.stderr = sys.stdout

if not is_quiet_mode and is_main_process:
    print(f"--- Démarrage de FragHub ---")
    print(f"CWD: {os.getcwd()}")
    print(f"sys.argv: {sys.argv}")

# Variables globales
loop = None
last_emit_time = 0
EMIT_THROTTLE = 0.05  # 20ms pour une fluidité maximale

# Configuration Socket.IO et FastAPI
# ping_timeout allongé : le moteur Rust garde parfois le verrou Python (GIL) plusieurs dizaines de secondes,
# ce qui empêche le serveur de répondre aux pings. Avec la valeur par défaut (20 s), l'interface croyait la
# connexion morte, se reconnectait, et les messages envoyés pendant la coupure étaient perdus.
sio = socketio.AsyncServer(async_mode='asgi', cors_allowed_origins='*', ping_interval=25, ping_timeout=300)

@asynccontextmanager
async def lifespan(app: FastAPI):
    global loop
    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        loop = asyncio.get_event_loop()
    yield

app = FastAPI(lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

socket_app = socketio.ASGIApp(sio, other_asgi_app=app)

class FragHubParams(BaseModel):
    input_directory: list
    input_db_names: dict = Field(default_factory=dict)
    output_directory: str
    normalize_intensity: float
    remove_peak_above_precursormz: float
    check_minimum_peak_requiered: float
    check_minimum_peak_requiered_n_peaks: float
    reduce_peak_list: float
    reduce_peak_list_max_peaks: float
    remove_spectrum_under_entropy_score: float
    remove_spectrum_under_entropy_score_value: float
    keep_mz_in_range: float
    keep_mz_in_range_from_mz: float
    keep_mz_in_range_to_mz: float
    check_minimum_of_high_peaks_requiered: float
    check_minimum_of_high_peaks_requiered_intensity_percent: float
    check_minimum_of_high_peaks_requiered_no_peaks: float
    calculate_de_novo: float
    de_novo_ppm_tolerance: float
    csv: float
    msp: float
    json_enabled: float = Field(alias='json')
    mzspeclib_json: float = Field(alias='mzspeclib_json', default=1.0)
    reset_updates: float
    # Journal de performances (PERF_LOGS) : 1.0 = écrit, 0.0 = non écrit (défaut). Ignoré par le moteur Rust.
    write_perf_logs: float = 0.0

    model_config = ConfigDict(populate_by_name=True)

def emit_to_frontend(event, data):
    global loop, last_emit_time

    # Throttling UNIQUEMENT sur progress — tous les autres événements
    # (total_items, prefix, step, completion, deletion…) passent toujours.
    # Throttler total_items causait des resets perdus → barre qui saute à 100%.
    if event == 'progress':
        current_time = time.time()
        if (current_time - last_emit_time) < EMIT_THROTTLE:
            return
        last_emit_time = current_time

    if loop:
        try:
            asyncio.run_coroutine_threadsafe(sio.emit(event, data), loop)
        except Exception:
            pass

# --- CALLBACKS ---
current_total_items = 0  # <-- Nouvelle variable globale

# État de la tâche en cours, renvoyé avec CHAQUE message de progression.
# Si un message 'total_items' / 'prefix' / 'item_type' se perd (reconnexion du socket), l'interface
# retrouve quand même la bonne tâche, son libellé, son total et son heure de début.
task_state = {'task': 0, 'total': 0, 'prefix': '', 'item_type': 'items', 'started': 0}

def task_meta():
    return dict(task_state)

def progress_callback(*args):
    global last_emit_time, current_total_items
    if global_stop_flag:
        raise Exception("Process stopped by user.")
    if args:
        perf_log.on_progress(args[0])  # PERF_LOG
        # Si on atteint 100%, on remet le chrono à zéro pour FORCER l'envoi
        if args[0] >= current_total_items:
            last_emit_time = 0

        emit_to_frontend('progress', {'value': args[0], **task_meta()})

def total_items_callback(*args):
    global last_emit_time, current_total_items
    last_emit_time = 0
    if args:
        perf_log.on_total(args[0])  # PERF_LOG
        current_total_items = args[0]  # <-- On enregistre le maximum
        task_state['task'] += 1
        task_state['total'] = args[0]
        task_state['started'] = time.time() * 1000  # ms, même horloge que Date.now() côté interface
        emit_to_frontend('total_items', task_meta())

def prefix_callback(*args):
    global last_emit_time
    last_emit_time = 0
    if args:
        perf_log.on_prefix(args[0])  # PERF_LOG
        task_state['prefix'] = args[0]
        emit_to_frontend('prefix', args[0])

def item_type_callback(*args):
    if args:
        perf_log.on_item_type(args[0])  # PERF_LOG
        task_state['item_type'] = args[0]
        emit_to_frontend('item_type', args[0])

def step_callback(*args):
    if args:
        perf_log.on_step(args[0])  # PERF_LOG
        emit_to_frontend('step', args[0])

def deletion_callback(*args):
    if args:
        perf_log.on_deletion(args[0])  # PERF_LOG
        emit_to_frontend('deletion', args[0])

def completion_callback(*args):
    if args:
        perf_log.on_completion(args[0])  # PERF_LOG
        if len(args) > 1:
            emit_to_frontend('completion', {'message': args[0], 'report_path': args[1]})
        else:
            emit_to_frontend('completion', {'message': args[0], 'report_path': None})

# --- GESTION DU STOP ---
global_stop_flag = False
def get_stop_flag(): return global_stop_flag

@app.get("/stop-analysis")
async def stop_analysis():
    global global_stop_flag
    global_stop_flag = True
    # Indicateur partagé du moteur Rust, regardé par toutes ses boucles : arrêt en moins d'une seconde, et
    # dossier de sortie remis comme avant le run (src/stop.rs, src/output_guard.rs)
    fraghub_rust.request_stop()
    return {"status": "stopped"}

@app.get("/health")
async def health_check():
    return {"status": "ok"}

@app.get("/report")
async def get_report(path: str):
    if os.path.exists(path):
        return FileResponse(path)
    return {"error": "Report not found"}

# --- BASE INTERNE : téléchargement au premier démarrage, suivi par l'écran de démarrage ---
# state : "idle" (pas encore regardé), "downloading", "ready" ou "error". message : texte à afficher.
database_status = {"state": "idle", "percent": 0.0, "message": ""}
database_status_lock = threading.Lock()

def _database_progress(phase, done, total):
    if phase == "download":
        percent = 100.0 * done / total if total else 0.0
        database_status.update(percent=percent, message="Downloading molecule database… %d %% of %.1f GB" % (percent, total / 1e9))
    elif phase == "verify":
        database_status.update(percent=100.0, message="Checking the downloaded database…")
    else:
        database_status.update(percent=100.0, message="Installing the molecule database…")

def _prepare_database():
    """Fil à part : télécharge la base interne absente, et note l'état pour /database-status."""
    try:
        internal_database.ensure_internal_database(BASE_DIR, progress=_database_progress)
        database_status.update(state="ready", message="")
    except internal_database.InternalDatabaseError as e:
        print(str(e), flush=True)   # message complet (cause, lien pour l'installer à la main) dans la console
        database_status.update(state="error", message="Database download failed. Check your connection and restart FragHub.")

@app.get("/database-status")
async def get_database_status():
    """
    État de la base interne. Au premier appel, lance son téléchargement si elle n'est pas sur le disque
    (code source sans la base ; l'exécutable installé la contient déjà) et répond tout de suite : l'écran de
    démarrage interroge cette adresse jusqu'à "ready" avant de demander /init-data.
    """
    with database_status_lock:
        if database_status["state"] == "idle":
            try:
                missing = internal_database.local_state(BASE_DIR, internal_database.read_reference(BASE_DIR)) == "absente"
            except internal_database.InternalDatabaseError as e:
                print(str(e), flush=True)
                database_status.update(state="error", message="Database reference file unreadable. Please reinstall FragHub.")
            else:
                if missing:
                    database_status.update(state="downloading", message="Downloading molecule database…")
                    threading.Thread(target=_prepare_database, daemon=True).start()
                else:
                    database_status.update(state="ready")
    return dict(database_status)

@app.get("/init-data")
async def init_data():
    if database_status["state"] == "downloading":
        raise HTTPException(status_code=503, detail="Database download in progress.")
    # Base interne absente (code source sans la base) : téléchargée ici, dans un fil à part pour que le serveur
    # continue de répondre pendant ce temps. Si elle est déjà là, l'appel ne fait que comparer des tailles.
    try:
        await asyncio.to_thread(internal_database.ensure_internal_database, BASE_DIR)
    except internal_database.InternalDatabaseError as e:
        print(str(e), flush=True)
        raise HTTPException(status_code=503, detail=str(e))
    perf_t0 = time.perf_counter()  # PERF_LOG
    fraghub_rust.load_internal_databases(BASE_DIR)
    perf_log.record_init(time.perf_counter() - perf_t0)  # PERF_LOG
    return {"status": "loaded"}

# --- EXÉCUTION ---
def execute_main_safely():
    try:
        # We need to capture the completion data from Rust so it doesn't close the GUI early
        rust_completion_data = None
        def proxy_completion_callback(msg, report_path=""):
            nonlocal rust_completion_data
            rust_completion_data = (msg, report_path)

        # Sortie mzSpecLib : le moteur Rust envoie les spectres par paquets à cet objet, qui les écrit
        # avec la bibliothèque mzspeclib (en parallèle), pendant l'étape d'écriture des sorties
        mzspeclib_sink = None
        if parameters_dict.get("mzspeclib_json", 0.0) == 1.0:
            mzspeclib_sink = mzspeclib_converter.MzSpecLibSink()

        # Appel direct de la fonction Rust
        try:
            fraghub_rust.main_orchestrator(
                parameters_dict,
                progress_callback,
                total_items_callback,
                prefix_callback,
                item_type_callback,
                step_callback,
                proxy_completion_callback,
                deletion_callback,
                get_stop_flag,
                mzspeclib_sink=mzspeclib_sink
            )
        finally:
            if mzspeclib_sink is not None:
                mzspeclib_sink.close(terminate=True)
        
        # Si l'utilisateur a cliqué sur stop ou s'il y a eu une erreur précoce dans Rust
        if rust_completion_data and rust_completion_data[0] == "Process stopped by user":
            if completion_callback:
                completion_callback(rust_completion_data[0], rust_completion_data[1])
            return

        # Une fois tout terminé, on envoie la vraie fin de process au GUI
        if completion_callback and rust_completion_data:
            completion_callback(rust_completion_data[0], rust_completion_data[1])

    except Exception as e:
        # Gestion de l'interruption utilisateur spécifiée dans Rust
        if str(e) == "Process stopped by user.":
            if deletion_callback:
                deletion_callback("\n-- PROCESS INTERRUPTED BY USER --")
            if completion_callback:
                completion_callback("Process stopped by user.")
        else:
            traceback.print_exc()
            if completion_callback:
                completion_callback(f"Error: {str(e)}")

def execute_main_safely_with_perf_log():  # PERF_LOG
    # Interrupteur "Write perf logs" de l'onglet Projects : sans lui, aucun journal n'est écrit
    # (les fonctions perf_log.on_* ne font rien tant qu'aucun run n'a été démarré avec perf_log.start).
    if parameters_dict.get("write_perf_logs", 0.0) != 1.0:  # PERF_LOG
        execute_main_safely()
        return
    perf_log.start(parameters_dict)  # PERF_LOG
    try:  # PERF_LOG
        execute_main_safely()  # PERF_LOG
    finally:  # PERF_LOG
        perf_log.finish()  # PERF_LOG

@app.post("/run-analysis")
async def run_analysis(params: FragHubParams, background_tasks: BackgroundTasks):
    global global_stop_flag
    global_stop_flag = False
    params_data = params.model_dump(by_alias=True)
    for key, value in params_data.items():
        parameters_dict[key] = value
    background_tasks.add_task(execute_main_safely_with_perf_log)  # PERF_LOG (avant : execute_main_safely)
    return {"status": "started"}

if __name__ == "__main__":
    multiprocessing.freeze_support()
    
    if is_cli_mode:
        import argparse
        parser = argparse.ArgumentParser(description="FragHub CLI Mode")
        parser.add_argument("--cli", action="store_true", help="Enable CLI mode")
        parser.add_argument("--quiet", action="store_true", help="Silence all progress bars and only print final success and total time")
        parser.add_argument("--input_directory", nargs='+', required=True, help="List of input files/directories")
        parser.add_argument("--output_directory", type=str, required=True, help="Output directory path")
        
        # Filtres on/off (yes/no)
        parser.add_argument("--normalize_intensity", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--remove_peak_above_precursormz", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--check_minimum_peak_requiered", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--reduce_peak_list", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--remove_spectrum_under_entropy_score", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--keep_mz_in_range", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--check_minimum_of_high_peaks_requiered", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--calculate_de_novo", type=str, choices=['yes', 'no'], default='no')
        parser.add_argument("--csv", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--msp", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--json", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--mzspeclib_json", type=str, choices=['yes', 'no'], default='yes')
        parser.add_argument("--reset_updates", type=str, choices=['yes', 'no'], default='no')
        parser.add_argument("--write_perf_logs", type=str, choices=['yes', 'no'], default='no',
                            help="Write a performance log in <output_directory>/PERF_LOGS (default: no, as in the GUI)")
        
        # Valeurs numériques (seuils, tolérances)
        parser.add_argument("--check_minimum_peak_requiered_n_peaks", type=float, default=3.0)
        parser.add_argument("--reduce_peak_list_max_peaks", type=float, default=500.0)
        parser.add_argument("--remove_spectrum_under_entropy_score_value", type=float, default=0.5)
        parser.add_argument("--keep_mz_in_range_from_mz", type=float, default=50.0)
        parser.add_argument("--keep_mz_in_range_to_mz", type=float, default=2000.0)
        parser.add_argument("--check_minimum_of_high_peaks_requiered_intensity_percent", type=float, default=5.0)
        parser.add_argument("--check_minimum_of_high_peaks_requiered_no_peaks", type=float, default=2.0)
        parser.add_argument("--de_novo_ppm_tolerance", type=float, default=10.0)
        
        args = parser.parse_args()
        
        # Résolution intelligente des chemins (fichiers et dossiers)
        resolved_files = []
        for path in args.input_directory:
            if os.path.isfile(path):
                resolved_files.append(os.path.abspath(path))
            elif os.path.isdir(path):
                # Parcourt tous les sous-dossiers à la recherche de fichiers valides
                for root, _, files in os.walk(path):
                    for file in files:
                        if file.lower().endswith(('.msp', '.mgf', '.csv', '.json', '.jsonl')):
                            resolved_files.append(os.path.abspath(os.path.join(root, file)))
            else:
                print(f"[WARNING] Le chemin spécifié est introuvable : {path}")

        if not resolved_files:
            print("\n[ERROR] Aucun fichier MS valide (.msp, .mgf, .csv, .json, .jsonl) trouvé dans les chemins spécifiés.")
            sys.exit(1)
            
        # Hydratation du parameters_dict avec les fichiers résolus et conversion yes/no
        args_dict = vars(args)
        
        # Conversion transparente yes/no en 1.0/0.0 pour Rust
        yes_no_keys = [
            'normalize_intensity', 'remove_peak_above_precursormz', 'check_minimum_peak_requiered',
            'reduce_peak_list', 'remove_spectrum_under_entropy_score', 'keep_mz_in_range',
            'check_minimum_of_high_peaks_requiered', 'calculate_de_novo', 'csv', 'msp', 'json', 'mzspeclib_json', 'reset_updates', 'write_perf_logs'
        ]
        for key in yes_no_keys:
            args_dict[key] = 1.0 if args_dict[key] == 'yes' else 0.0
            
        args_dict['input_directory'] = resolved_files
        args_dict['input_db_names'] = {}
        parameters_dict.update(args_dict)
        
        if not args.quiet:
            print("\n========================================")
            print("          FragHub CLI Mode Actif          ")
            print("========================================\n")
        
        # Base interne absente (code source sans la base) : téléchargée depuis Zenodo avant le chargement
        try:
            internal_database.ensure_internal_database(BASE_DIR)
        except internal_database.InternalDatabaseError as e:
            print(str(e), file=sys.stderr)
            sys.exit(1)

        # Chargement initial des bases
        perf_t0 = time.perf_counter()  # PERF_LOG
        fraghub_rust.load_internal_databases(BASE_DIR)
        perf_log.record_init(time.perf_counter() - perf_t0)  # PERF_LOG
        
        global_start_time = time.time()
        
        # Variables pour la barre de progression en console
        cli_total_items = [0]
        cli_current_prefix = [""]
        cli_start_time = [0.0]
        
        def cli_progress_callback(*cb_args):
            if cb_args: perf_log.on_progress(cb_args[0])  # PERF_LOG
            if args.quiet or not cb_args: return
            val = cb_args[0]
            total = cli_total_items[0]
            start_t = cli_start_time[0]
            if total > 0:
                percent = val / total
                bar_length = 30
                filled_length = int(bar_length * percent)
                bar = '█' * filled_length + '░' * (bar_length - filled_length)
                
                # Calculs de temps et vitesse
                elapsed = time.time() - start_t
                speed = val / elapsed if elapsed > 0 else 0
                remaining = (total - val) / speed if speed > 0 else 0
                
                def format_time(seconds):
                    if seconds == float('inf') or seconds < 0: return "--:--"
                    m, s = divmod(int(seconds), 60)
                    h, m = divmod(m, 60)
                    if h > 0: return f"{h:02d}:{m:02d}:{s:02d}"
                    return f"{m:02d}:{s:02d}"
                
                # Couleurs ANSI (Bleu clair = en cours, Vert = terminé)
                color = '\033[96m' if val < total else '\033[92m'
                reset = '\033[0m'
                
                stats = f"[{format_time(elapsed)} < {format_time(remaining)}, {speed:.1f} it/s]"
                
                # \033[K permet d'effacer le reste de la ligne pour éviter les artefacts visuels
                print(f"\r  {color}[{bar}]{reset} {percent*100:.1f}% | {val}/{total} {stats}\033[K", end="", flush=True)
                if val >= total:
                    print() # Nouvelle ligne à 100%
                    
        def cli_total_items_callback(*cb_args):
            if cb_args: perf_log.on_total(cb_args[0])  # PERF_LOG
            if cb_args:
                cli_total_items[0] = cb_args[0]
                cli_start_time[0] = time.time() # Reset chrono au début de chaque étape
            
        def cli_prefix_callback(*cb_args):
            if cb_args: perf_log.on_prefix(cb_args[0])  # PERF_LOG
            if args.quiet or not cb_args: return
            prefix = cb_args[0]
            cli_current_prefix[0] = prefix
            # Flèche de tâche et texte en blanc/gris
            print(f"\n\033[1;37m▶ {prefix}\033[0m")
            
        def cli_item_type_callback(*cb_args):
            if cb_args: perf_log.on_item_type(cb_args[0])  # PERF_LOG
            pass # Non nécessaire en CLI
            
        def cli_step_callback(*cb_args):
            if cb_args: perf_log.on_step(cb_args[0])  # PERF_LOG
            if args.quiet or not cb_args: return
            step = cb_args[0]
            # Étape principale en jaune et en gras
            print(f"\n\033[1;33m=== {step} ===\033[0m")
            
        def cli_completion_callback(*cb_args):
            if cb_args: perf_log.on_completion(cb_args[0])  # PERF_LOG
            if args.quiet or not cb_args: return
            msg = cb_args[0]
            # Message de fin en vert
            print(f"\n\033[1;32m✔ {msg}\033[0m\n")
            
        def cli_deletion_callback(*cb_args):
            if cb_args: perf_log.on_deletion(cb_args[0])  # PERF_LOG
            if args.quiet or not cb_args: return
            msg = cb_args[0]
            # Rapports de suppression en rouge/orange
            print(f"\033[38;5;208m[REPORT] {msg}\033[0m")
            
        def cli_get_stop_flag():
            return False # Pas d'interruption via UI en mode CLI (l'utilisateur fera Ctrl+C)

        with_mzspeclib = parameters_dict.get("mzspeclib_json", 0.0) == 1.0

        # Journal de performances seulement si --write_perf_logs yes (comme l'interrupteur de l'onglet Projects).
        # Sinon perf_log n'est pas démarré : ses fonctions ne font rien et finish() n'écrit aucun fichier.
        if parameters_dict.get("write_perf_logs", 0.0) == 1.0:  # PERF_LOG
            perf_log.start(parameters_dict)  # PERF_LOG
        # Sortie mzSpecLib, comme dans l'interface (execute_main_safely) : le moteur Rust envoie les spectres
        # à cet objet, qui écrit <output_directory>/mzSpecLib pendant l'étape d'écriture des sorties.
        mzspeclib_sink = mzspeclib_converter.MzSpecLibSink() if with_mzspeclib else None
        # Ctrl+C : arrêt propre par le moteur Rust (dossier de sortie remis comme avant le run) ; un deuxième Ctrl+C
        # quitte tout de suite. Avant, Ctrl+C n'était vu qu'au retour du moteur dans Python.
        fraghub_rust.install_ctrl_c_handler()
        try:
            # Lancement de l'orchestrateur de façon synchrone dans le thread principal
            fraghub_rust.main_orchestrator(
                parameters_dict,
                cli_progress_callback,
                cli_total_items_callback,
                cli_prefix_callback,
                cli_item_type_callback,
                cli_step_callback,
                cli_completion_callback,
                cli_deletion_callback,
                cli_get_stop_flag,
                mzspeclib_sink=mzspeclib_sink
            )
        except Exception as e:
            if str(e) == "Process stopped by user.":
                print("\n[STOP] Traitement arrêté : le dossier de sortie a été remis comme avant le run.")
                perf_log.finish("Arrêté par l'utilisateur")  # PERF_LOG
                sys.exit(130)
            print(f"\n[ERROR] Une erreur s'est produite : {e}")
            perf_log.finish(f"Erreur : {e}")  # PERF_LOG
            sys.exit(1)
        finally:
            if mzspeclib_sink is not None:
                mzspeclib_sink.close(terminate=True)

        perf_log.finish()  # PERF_LOG
            
        # Calcul et affichage du temps total
        total_time = time.time() - global_start_time
        def format_total(seconds):
            m, s = divmod(int(seconds), 60)
            h, m = divmod(m, 60)
            if h > 0: return f"{h}h {m}m {s}s"
            if m > 0: return f"{m}m {s}s"
            return f"{s}s"
            
        if args.quiet:
            print(f"FragHub processed successfully in {format_total(total_time)}.")
        else:
            print(f"\n\033[1;32m★ All tasks completed successfully in {format_total(total_time)} ★\033[0m\n")
            
        sys.exit(0)
    else:
        # Lancement normal du GUI (FastAPI / Uvicorn / WebSockets)
        uvicorn.run(socket_app, host="127.0.0.1", port=8000)