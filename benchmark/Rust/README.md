# Benchmark — FragHub Rust (2.0.0, `Rust_FragHub` branch)

Full per-task measurements of the Rust version of FragHub (Rust engine through PyO3, polars, rayon, RDKit C++).
The comparison with the Python version is in [../README.md](../README.md).

## GNPS JSON run (25 November 2026)

| | |
|---|---|
| Code | `Rust_FragHub` branch (version 2.0.0) |
| Input | `ALL_GNPS.json` — 1 929 MB, 418 054 spectra |
| Filters | default values, de novo off, outputs CSV + MSP + JSON (mzSpecLib off: the Python version has no such output), `reset_updates` on |
| Internal database loading (before the run) | **3.3 s** (RSS 4 038 MB) |
| Run duration | **68.2 s (1 min 08 s)** |
| Max memory during the run | **6 992 MB** |
| Spectra written | **202 651** |
| Unique InChIKeys | 95 809 |

The RDKit step uses the precomputed results stored in the internal database (RDKit cache) for the molecules it already knows.

Raw logs: [perf_gnps_json.txt](perf_gnps_json.txt) · [perf_gnps_json.csv](perf_gnps_json.csv)

### Command

```powershell
git switch Rust_FragHub
$env:FRAGHUB_PERF_LABEL = "rust-gnps"
.venv\Scripts\python scripts\FragHub.py --cli --input_directory <path>\ALL_GNPS.json --output_directory <path>\OUT_BENCH_RUST --reset_updates yes --mzspeclib_json no --write_perf_logs yes
```

### Per-task detail

| # | Step | Task | Items | Unit | Done | Start (s) | Duration (s) | RSS end (MB) | RSS max (MB) | CPU cores |
|---:|---|---|---:|---|---:|---:|---:|---:|---:|---:|
| 0 | Startup | Loading internal databases (before the run) |  |  |  |  | 3.30 | 4038 |  |  |
| 1 | PARSING JSON TO DICT | loading [ALL_GNPS.json] | 2 022 385 992 | bytes | 2 022 385 992 | 0.02 | 3.07 | 6084 | 6084 | 0.9 |
| 2 | PARSING JSON TO DICT | Parsing JSON spectrums | 418 054 | spectra | 418 054 | 3.08 | 4.92 | 6779 | 6779 | 2.8 |
| 3 | Generating SPLASH IDs | Generating SPLASH IDs for [JSON] | 418 054 | spectra | 418 054 | 8.02 | 0.62 | 6700 | 6779 | 7.0 |
| 4 | Generating SPLASH IDs | Merging files (Please wait)... | 1 |  | 1 | 8.64 | 0.13 | 6719 | 6719 | 0.1 |
| 5 | Checking for Updates | Checking for updates | 418 054 | spectra | 418 054 | 8.78 | 0.17 | 6734 | 6735 | 0.9 |
| 6 | Checking for Updates | Saving updates and logs... |  |  |  | 8.95 | 0.44 | 6729 | 6741 | 0.3 |
| 7 | Cleaning Spectrums | cleaning spectrums | 418 054 | spectra | 418 054 | 9.40 | 3.06 | 6065 | 6745 | 9.3 |
| 8 | Cleaning Spectrums |  | 191 966 | rows | 191 966 | 12.47 | 0.03 | 6086 | 6086 | 10.3 |
| 9 | RDKit Derivation and Mass Calculation | derivation and calculation (RDKit via Rust) | 226 088 | rows |  | 12.51 | 0.09 | 5978 | 6086 | 5.6 |
| 10 | RDKit Derivation and Mass Calculation | RDKit calculation | 135 889 |  | 135 889 | 12.59 | 46.70 | 6085 | 6992 | 15.0 |
| 11 | RDKit Derivation and Mass Calculation | Checking failures & fallback PubChem (Pass 2)... |  |  |  | 59.29 | 0.35 | 6085 | 6085 | 1.0 |
| 12 | RDKit Derivation and Mass Calculation | Processing molecules (Pass 2)... | 22 |  | 22 | 59.64 | 0.70 | 6031 | 6107 | 2.0 |
| 13 | RDKit Derivation and Mass Calculation | applying RDKit results to all spectra... | 226 088 |  | 226 088 | 60.34 | 0.76 | 6076 | 6076 | 1.0 |
| 14 | Mass & Adduct Correction | Mass & Adduct Correction | 225 948 | spectra | 225 948 | 61.11 | 0.99 | 5781 | 6078 | 12.2 |
| 15 | Removing Duplicates | Removing duplicates | 213 571 | spectra | 213 571 | 62.11 | 0.86 | 5877 | 5877 | 5.0 |
| 16 | Completing from PubChem Datas | Enriching data from PubChem | 202 651 | rows | 202 651 | 62.98 | 0.16 | 5847 | 5877 | 13.1 |
| 17 | Ontologies Completion | Updating ontologies | 202 651 | rows | 202 651 | 63.14 | 0.13 | 5847 | 5847 | 7.3 |
| 18 | Splitting Spectrums | Splitting spectra into categories | 202 651 | spectra | 202 651 | 63.28 | 0.04 | 5925 | 5925 | 12.7 |
| 19 | Writing CSV | Writing CSV files | 202 651 | spectra | 189 | 63.34 | 0.01 | 5944 | 5944 | 14.5 |
| 20 | Writing CSV | Writing NEG_DI.csv to disk... |  |  |  | 63.35 | 0.00 | 5949 | 5949 | 50.0 |
| 21 | Writing CSV | Writing POS_DI.csv to disk... |  |  | 25 177 | 63.35 | 0.19 | 6119 | 6119 | 14.7 |
| 22 | Writing CSV | Writing NEG_LC.csv to disk... |  |  | 202 651 | 63.54 | 0.30 | 6175 | 6175 | 4.4 |
| 23 | Writing CSV | Writing POS_LC.csv to disk... |  |  | 202 651 | 63.85 | 0.01 | 6175 | 6175 | 0.0 |
| 24 | Writing MSP | Writing MSP files | 202 651 | spectra | 202 651 | 63.87 | 0.59 | 6189 | 6189 | 7.0 |
| 25 | Writing JSON | Writing JSON files | 202 651 | spectra | 14 | 64.47 | 0.00 | 6180 | 6189 | 0.0 |
| 26 | Writing JSON | Writing NEG_DI.json to disk... |  |  | 189 | 64.48 | 0.03 | 6133 | 6180 | 17.8 |
| 27 | Writing JSON | Writing POS_DI.json to disk... |  |  | 25 177 | 64.51 | 0.58 | 6308 | 6308 | 16.0 |
| 28 | Writing JSON | Writing NEG_LC.json to disk... |  |  | 202 651 | 65.09 | 0.98 | 6232 | 6308 | 4.8 |
| 29 | Writing JSON | Writing POS_LC.json to disk... |  |  | 202 651 | 66.07 | 0.01 | 6232 | 6232 | 0.0 |

In the writing steps, the "Done" column of the per-file lines is shifted by one line (the counter of a file is reported on the next line); durations are not affected.

## Earlier screenshots

![img.png](img.png)
![img_1.png](img_1.png)
![img_2.png](img_2.png)
![img_14.png](img_14.png)
![img_4.png](img_4.png)
![img_5.png](img_5.png)
![img_6.png](img_6.png)

MoNA run: [mona_rust.md](mona_rust.md)
