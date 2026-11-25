# Benchmark — FragHub Python (1.5, `develop` branch)

Full per-task measurements of the old Python version of FragHub (pandas, numba, RDKit for Python).
The comparison with the Rust version is in [../README.md](../README.md).

## GNPS JSON run (25 November 2026)

| | |
|---|---|
| Code | `benchmark` branch (copy of `develop`, commit `7d0abe0`) + `scripts/bench_cli.py` |
| Input | `ALL_GNPS.json` — 1 929 MB, 418 054 spectra |
| Filters | default values (same as the Rust CLI), de novo off, outputs CSV + MSP + JSON, `reset_updates` on |
| Internal databases loading (before the run) | **19.6 s** (RSS 4 398 MB) |
| Run duration | **481.6 s (8 min 02 s)** |
| Max memory during the run | **11 627 MB** |
| Spectra written | **200 366** (POS LC 165 952, NEG LC 34 410, POS LC in silico 4) |
| Unique InChIKeys | 74 544 |

Raw logs: [perf_gnps_json.txt](perf_gnps_json.txt) · [perf_gnps_json.csv](perf_gnps_json.csv)

### Command

The old version has no CLI (it only runs behind the FastAPI server driven by the interface).
`scripts/bench_cli.py` on the `benchmark` branch does what the server does (loads the internal databases,
then runs `MAIN` with the same parameters) and records every step with the same `perf_log.py` as the Rust version.

```powershell
git switch benchmark
py -3.12 -m venv scratch\venv_develop
scratch\venv_develop\Scripts\python -m pip install -r requirements.txt
$env:FRAGHUB_PERF_LABEL = "python-develop-gnps"
scratch\venv_develop\Scripts\python scripts\bench_cli.py --input_directory <path>\ALL_GNPS.json --output_directory <path>\OUT_BENCH_PYTHON --reset_updates yes
```

### Per-task detail

| # | Step | Task | Items | Unit | Done | Start (s) | Duration (s) | RSS end (MB) | RSS max (MB) | CPU cores |
|---:|---|---|---:|---|---:|---:|---:|---:|---:|---:|
| 0 | Startup | Loading internal databases (before the run) |  |  |  |  | 19.61 | 4398 |  |  |
| 1 | PARSING JSON TO DICT | loading [ALL_GNPS.json] | 418 054 | spectra | 418 054 | 0.02 | 15.83 | 8801 | 8801 | 1.0 |
| 2 | PARSING JSON TO DICT | Parsing JSON spectrums | 418 054 | spectra | 418 054 | 15.85 | 49.47 | 11317 | 11356 | 1.0 |
| 3 | GENERATING SPLASH UNIQUE ID | generating SPLASH for [JSON] | 418 054 | spectra | 418 054 | 65.33 | 37.28 | 11128 | 11596 | 1.0 |
| 4 | REMOVING DUPLICATAS | Removing duplicates | 418 054 | spectra | 100 | 102.62 | 50.13 | 11276 | 11627 | 1.0 |
| 5 | CHECKING FOR UPDATES | checking for updates | 330 398 | spectra | 330 398 | 152.76 | 3.98 | 11296 | 11308 | 0.9 |
| 6 | CLEANING SPECTRUMS | cleaning spectrums | 330 398 | spectra | 330 398 | 156.75 | 103.44 | 9168 | 11359 | 1.0 |
| 7 | MOLS DERIVATION AND MASS CALCULATION | derivation and calculation | 175 648 | rows | 175 648 | 260.20 | 138.33 | 9189 | 9189 | 1.0 |
| 8 | MOLS DERIVATION AND MASS CALCULATION | updating rows | 229 925 |  | 229 925 | 398.53 | 8.20 | 9240 | 9292 | 1.0 |
| 9 | COMPLETING FROM PUBCHEM DATAS | enriching data from PubChem | 200 585 | rows | 200 585 | 406.73 | 4.28 | 9244 | 9990 | 1.0 |
| 10 | ONTOLOGIES COMPLETION | updating ontologies | 74 544 | rows | 74 544 | 411.05 | 1.22 | 9243 | 9416 | 1.0 |
| 11 | SPLITTING [POS / NEG] | Splitting POS/NEG | 200 585 | rows | 200 585 | 412.28 | 0.11 | 9297 | 9297 | 0.9 |
| 12 | SPLITTING [LC / GC] | Splitting LC/GC | 200 366 | rows | 200 366 | 412.40 | 0.16 | 9349 | 9349 | 0.9 |
| 13 | SPLITTING [EXP / In-Silico] | POS_LC_In_Silico | 165 956 | rows | 4 | 412.56 | 0.00 | 9349 | 9349 | 0.0 |
| 14 | SPLITTING [EXP / In-Silico] | POS_LC_Exp | 165 956 | rows | 165 952 | 412.57 | 0.06 | 9393 | 9393 | 1.0 |
| 15 | SPLITTING [EXP / In-Silico] | NEG_LC_In_Silico | 34 410 | rows | 0 | 412.63 | 0.00 | 9393 | 9393 | 0.0 |
| 16 | SPLITTING [EXP / In-Silico] | NEG_LC_Exp | 34 410 | rows | 34 410 | 412.63 | 0.05 | 9350 | 9393 | 0.6 |
| 17 | CONVERTING CSV TO MSP | (between two tasks) |  |  |  | 412.68 | 0.33 | 9393 | 9393 |  |
| 18 | CONVERTING CSV TO MSP | Formatting POS_LC to MSP | 165 952 | rows | 165 952 | 413.02 | 11.17 | 9701 | 9744 | 1.0 |
| 19 | CONVERTING CSV TO MSP | Formatting POS_LC_insilico to MSP | 4 | rows | 4 | 424.18 | 0.10 | 9701 | 9701 | 0.0 |
| 20 | CONVERTING CSV TO MSP | Formatting POS_GC to MSP | 0 | rows |  | 424.28 | 0.10 | 9701 | 9701 | 0.0 |
| 21 | CONVERTING CSV TO MSP | Formatting POS_GC_insilico to MSP | 0 | rows |  | 424.38 | 0.15 | 9710 | 9710 | 0.3 |
| 22 | CONVERTING CSV TO MSP | Formatting NEG_LC to MSP | 34 410 | rows | 34 410 | 424.53 | 2.39 | 9773 | 9773 | 1.0 |
| 23 | CONVERTING CSV TO MSP | Formatting NEG_LC_insilico to MSP | 0 | rows |  | 426.92 | 0.10 | 9773 | 9773 | 0.0 |
| 24 | CONVERTING CSV TO MSP | Formatting NEG_GC to MSP | 0 | rows |  | 427.02 | 0.10 | 9773 | 9773 | 0.0 |
| 25 | CONVERTING CSV TO MSP | Formatting NEG_GC_insilico to MSP | 0 | rows |  | 427.12 | 0.01 | 9773 | 9773 | 0.0 |
| 26 | WRITING CSV | (between two tasks) |  |  |  | 427.13 | 0.30 | 9992 | 9992 |  |
| 27 | WRITING CSV | Writing POS_LC.csv to CSV | 165 952 | rows | 165 952 | 427.43 | 0.37 | 9994 | 9994 | 0.7 |
| 28 | WRITING CSV | Writing POS_GC.csv to CSV | 0 | rows |  | 427.80 | 0.13 | 10021 | 10021 | 0.4 |
| 29 | WRITING CSV | Writing NEG_LC.csv to CSV | 34 410 | rows | 34 410 | 427.93 | 0.22 | 10022 | 10022 | 0.5 |
| 30 | WRITING CSV | Writing NEG_GC.csv to CSV | 0 | rows |  | 428.15 | 0.10 | 10022 | 10022 | 0.0 |
| 31 | WRITING CSV | Writing POS_LC_In_Silico.csv to CSV | 4 | rows | 4 | 428.25 | 0.20 | 9784 | 10022 | 0.5 |
| 32 | WRITING CSV | Writing POS_GC_In_Silico.csv to CSV | 0 | rows |  | 428.45 | 0.10 | 9784 | 9784 | 0.0 |
| 33 | WRITING CSV | Writing NEG_LC_In_Silico.csv to CSV | 0 | rows |  | 428.55 | 0.10 | 9784 | 9784 | 0.0 |
| 34 | WRITING CSV | Writing NEG_GC_In_Silico.csv to CSV | 0 | rows |  | 428.65 | 0.01 | 9784 | 9784 | 0.0 |
| 35 | WRITING MSP | Writing POS_LC.msp to MSP | 165 952 | spectra | 165 952 | 428.78 | 0.93 | 9784 | 9784 | 0.9 |
| 36 | WRITING MSP | Writing POS_LC_insilico.msp to MSP | 4 | spectra | 4 | 429.71 | 0.10 | 9784 | 9784 | 0.0 |
| 37 | WRITING MSP | Writing POS_GC.msp to MSP | 0 | spectra |  | 429.81 | 0.10 | 9784 | 9784 | 0.0 |
| 38 | WRITING MSP | Writing POS_GC_insilico.msp to MSP | 0 | spectra |  | 429.91 | 0.10 | 9784 | 9784 | 0.0 |
| 39 | WRITING MSP | Writing NEG_LC.msp to MSP | 34 410 | spectra | 34 410 | 430.01 | 0.25 | 9784 | 9784 | 0.6 |
| 40 | WRITING MSP | Writing NEG_LC_insilico.msp to MSP | 0 | spectra |  | 430.27 | 0.10 | 9784 | 9784 | 0.0 |
| 41 | WRITING MSP | Writing NEG_GC.msp to MSP | 0 | spectra |  | 430.37 | 0.10 | 9784 | 9784 | 0.0 |
| 42 | WRITING MSP | Writing NEG_GC_insilico.msp to MSP | 0 | spectra |  | 430.47 | 0.01 | 9784 | 9784 | 0.0 |
| 43 | WRITING JSON | Writing POS_LC.json to JSON | 165 952 | rows | 165 952 | 430.59 | 43.57 | 9787 | 9947 | 1.0 |
| 44 | WRITING JSON | Writing POS_GC.json to JSON | 0 | rows |  | 474.16 | 0.10 | 9787 | 9787 | 0.0 |
| 45 | WRITING JSON | Writing NEG_LC.json to JSON | 34 410 | rows | 34 410 | 474.26 | 6.59 | 9787 | 9795 | 1.0 |
| 46 | WRITING JSON | Writing NEG_GC.json to JSON | 0 | rows |  | 480.85 | 0.10 | 9787 | 9787 | 0.0 |
| 47 | WRITING JSON | Writing POS_LC_In_Silico.json to JSON | 4 | rows | 4 | 480.96 | 0.11 | 9787 | 9787 | 0.0 |
| 48 | WRITING JSON | Writing POS_GC_In_Silico.json to JSON | 0 | rows |  | 481.06 | 0.10 | 9787 | 9787 | 0.0 |
| 49 | WRITING JSON | Writing NEG_LC_In_Silico.json to JSON | 0 | rows |  | 481.16 | 0.10 | 9787 | 9787 | 0.0 |
| 50 | WRITING JSON | Writing NEG_GC_In_Silico.json to JSON | 0 | rows |  | 481.27 | 0.36 | 7156 | 9787 | 1.0 |

## Earlier screenshots

![img.png](img.png)
![img_1.png](img_1.png)
![img_2.png](img_2.png)
![img_3.png](img_3.png)
![img_4.png](img_4.png)
![img_5.png](img_5.png)
![img_6.png](img_6.png)
![img_7.png](img_7.png)
![img_8.png](img_8.png)
![img_9.png](img_9.png)
![img_10.png](img_10.png)

MoNA run: [mona_python.md](mona_python.md)
