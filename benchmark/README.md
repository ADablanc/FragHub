# FragHub benchmark — Python (1.5) vs Rust (2.0.0)

Same input file, same filters, same PC (Windows), runs one after the other on 25 November 2026.
Both versions record every step with the same module (`scripts/perf_log.py`): duration, memory (RSS) and average number of CPU cores used.

- [Python/](Python/README.md) — old version (`develop` branch: pandas, numba, RDKit for Python), full per-task detail
- [Rust/](Rust/README.md) — new version (`Rust_FragHub` branch: Rust engine, polars, rayon, RDKit C++), full per-task detail

## Input

`ALL_GNPS.json` (GNPS, JSON format) — **1 929 MB, 418 054 spectra**.
Default filters, de novo calculation off, outputs CSV + MSP + JSON (mzSpecLib off on the Rust side: the Python version has no such output), `reset_updates` on.

## Totals

| | Python | Rust | Coefficient |
|---|---:|---:|---:|
| **Run (input file → written files)** | **481.6 s (8 min 02 s)** | **68.2 s (1 min 08 s)** | **× 7.1** |
| Internal databases loading (at startup) | 19.6 s | 3.3 s | × 5.9 |
| **Overall total (startup + run)** | **501.2 s (8 min 21 s)** | **71.5 s (1 min 12 s)** | **× 7.0** |
| Max memory during the run | 11 627 MB | 6 992 MB | × 1.7 (less memory) |
| Throughput | 868 spectra/s | 6 130 spectra/s | × 7.1 |

Coefficient = Python time ÷ Rust time (for memory: Python ÷ Rust). Above 1, the Rust version is faster (or uses less memory).

## Step by step

The two versions do not split their work the same way, so tasks are grouped into common phases.

| Phase | Python | Rust | Coefficient | CPU cores Python / Rust |
|---|---:|---:|---:|---:|
| Internal databases loading (before the run) | 19.6 s | 3.3 s | × 5.9 | – |
| Reading the JSON file | 65.30 s | 7.99 s | × 8.2 | 1.0 / 2.8 |
| SPLASH identifiers | 37.28 s | 0.75 s | × 50 | 1.0 / 7.0 |
| Duplicate removal | 50.13 s | 0.86 s | × 58 | 1.0 / 5.0 |
| Update check | 3.98 s | 0.61 s | × 6.5 | 0.9 / 0.9 |
| Spectrum cleaning | 103.44 s | 3.09 s | × 33 | 1.0 / 9.3 |
| Molecules (RDKit derivation and masses) | 146.53 s | 48.60 s | × 3.0 | 1.0 / 15.0 |
| Mass and adduct correction (Rust only) | – | 0.99 s | – | – / 12.2 |
| PubChem completion | 4.28 s | 0.16 s | × 27 | 1.0 / 13.1 |
| Ontologies completion | 1.22 s | 0.13 s | × 9.4 | 1.0 / 7.3 |
| Splitting (POS/NEG, LC/GC, exp./in silico) | 0.38 s | 0.04 s | × 9.5 | 1.0 / 12.7 |
| MSP (formatting + writing) | 16.14 s | 0.59 s | × 27 | 1.0 / 7.0 |
| CSV writing | 1.53 s | 0.51 s | × 3.0 | 0.5 / 5 |
| JSON writing | 51.03 s | 1.60 s | × 32 | 1.0 / 5 |
| Other (time between tasks, report) | 0.36 s | 2.28 s | | |

- The old version does use multithreading (`concurrent.futures.ThreadPoolExecutor` for parsing, SPLASH, cleaning, MSP conversion…), but its threads run Python code and are held back by Python's global interpreter lock (GIL): the measured average stays around one CPU core. The Rust version runs its threads in parallel without that lock (rayon) and spreads most steps over all cores.
- **The molecule step (RDKit) is now the main cost of the Rust run**: 48.6 s out of 68.2 s (71 %). It is also where the gain is smallest (× 3.0).
  The Rust version reuses the RDKit results stored in its internal database (RDKit cache), which the Python version does not have.

## Results

The two versions do not apply exactly the same rules, so the outputs differ.

| | Python | Rust |
|---|---:|---:|
| Input spectra | 418 054 | 418 054 |
| **Spectra written** | **200 366** | **202 651** |
| Unique InChIKeys | 74 544 | 95 809 |
| Duplicates removed | 87 656 | 10 920 |
| No SMILES, InChI or InChIKey (after recalculation) | 69 468 | 82 612 |
| Low entropy score | 619 | 72 730 |
| Minimum number of peaks not reached | 14 736 | 18 062 |
| Minimum number of high peaks not reached | 13 917 | 15 246 |
| No or wrong adduct | 1 266 | 1 244 |
| All peaks above precursor m/z | 342 | 1 826 |
| No peaks in m/z range | 114 | 226 |
| No peak list | 1 | 160 |
| No precursor m/z | 10 | 0 |
| Precursor m/z matching no adduct (Rust only) | – | 1 843 |
| Total deleted | 186 863 | 215 403 |

Main differences:
- the Rust version removes duplicates after the molecule calculation (on the recalculated structures), not before;
- the entropy score is computed differently;
- the Rust version has extra checks (precursor m/z matching no adduct) and an extra category for direct infusion (DI).

## How to reproduce

See the commands in [Python/README.md](Python/README.md) and [Rust/README.md](Rust/README.md).
