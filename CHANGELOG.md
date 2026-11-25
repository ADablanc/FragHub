# Change Log
All notable changes to this project will be documented in this file.

- **19_06_2026** (Version 2.0.0 Major Update):
  - **Rust Core Engine**: Completely rewrote the data processing pipeline (parsers, molecule derivation, filters, writers) in Rust via PyO3, replacing the pure Python implementation for massively increased high-throughput processing speed.
  - **Headless CLI Mode**: Introduced a new `--cli` mode to run FragHub without the graphical interface, ideal for servers and automated data pipelines. Features advanced real-time metrics (ETA, speed) and a `--quiet` flag.
  - **Recursive Directory Parsing**: Added input resolution in CLI mode to automatically scan entire folder structures and batch-process valid MS files.
  - **Backend Modernization**: Updated the FastAPI Python backend to use modern `lifespan` handlers, resolving deprecation warnings, and upgraded Pydantic schemas to v2 standards (`ConfigDict`).
  - **Agnostic Callback System**: Built a robust event bridge allowing the Rust engine to safely stream asynchronous progress logs to both the CLI terminal and the Electron/Nuxt GUI via WebSockets.
  - **Toolchain Migration**: Transitioned the entire Python environment and dependency management to `uv`, significantly speeding up setup and resolving packaging constraints with PyInstaller across OS environments.
  - **Interactive Reporting**: Developed a comprehensive HTML execution report incorporating interactive Sunburst and UpSetPlot charts that summarize dataset evolution after processing.
  - **UI/UX & Logs Polishing**: Disabled the automatic launch of Electron Developer Tools at startup, unified the application icons, and drastically cleaned/standardized all the real-time progress steps displayed during processing.
  - **Packaging Fixes**: Resolved Python multiprocessing freeze issues when packaging the Rust-RDKit bridge with PyInstaller, and updated Electron metadata.
  - **RDKit C++**: molecule derivation and exact masses are computed by RDKit C++ called directly from Rust, on all cores; RDKit Python is no longer used nor bundled.
  - **License**: FragHub is now distributed under the CeCILL 2.1 license (`LICENSE`), replacing CC BY-NC 4.0. `THIRD_PARTY_LICENSES.txt` is shipped with the installer; ClassyFire terms of use and citations stated in the README.
  - **Input files**: case-insensitive extensions, `.jsonl`, NIST multi-peak MSP lines, decimal comma in peak m/z, and many metadata reading fixes (GNPS, MoNA, MassBank, MS-DIAL, MSnLib, Spectraverse): more fields read, no value lost or overwritten.
  - **Collision energy**: also read from names and comments, and written the same way everywhere (unit kept, `NCE`, `Ramp` / `Stepped`).
  - **Fragmentation method**: read from the instrument, origin or name when its field is empty.
  - **Instruments**: instrument tree regenerated from the corrected spreadsheet; LC/GC completed from the ionization; better search when several brands are cited; `QQ` searched as `QQQ`.
  - **Direct injection**: new `POS_DI` / `NEG_DI` libraries; MALDI, FAB and FD spectra are DI.
  - **Molecule normalization**: standard InChI / InChIKey of the desalted, neutralized molecule; SMILES written as the RDKit canonical tautomer; the spectrum's own SMILES is tried before the database's; InChI mixtures of same-formula components (`2C…`) are desalted with the m/z of each spectrum.
  - **RDKit cache**: rows of the internal database marked `RDKIT_VERSION` are reused instead of being recomputed, with identical results (`FRAGHUB_RDKIT_CACHE=0` turns it off).
  - **Internal database**: re-normalized like the spectra, NPClassifier classes recomputed, one row per InChIKey, 325 112 molecules added, names cleaned with PubChem. Version 2026-11-25 published on Zenodo ([10.5281/zenodo.24535407](https://doi.org/10.5281/zenodo.24535407)), downloaded at first start and updated with FragHub.
  - **Completion from the internal database**: the name always comes from the database; InChI, SMILES, formula and masses only fill empty fields.
  - **Duplicates**: the spectrum kept gets the values of its removed duplicates for its empty fields.
  - **Deleted spectra**: GNPS "suspect" spectra (structure of a related compound) are deleted (`suspect_related_structure.csv`); spectra without peaks are reported in `DELETED_SPECTRUMS` instead of being dropped silently.
  - **Names**: annotation notes such as `(known isomers: N; isobaric peaks: M)` moved from the name to COMMENT.
  - **Outputs**: SPLASH computed on the written peaks; computed values written with at most 8 decimals; de novo formulas of negative spectra written with `-`; mzSpecLib output rewritten with PSI-MS terms.
  - **GC spectra without ion mode** set to positive, except in negative chemical ionization.
  - **Lower memory use, same results**: memory peak 29.9 GB -> 20.2 GB on the full test set.
  - **Stop button and Ctrl+C** stop a run within about a second and restore the output folder.
  - **Interface**: invalid fields shown and blocking START, server errors shown, no duplicate input file, source database guessed from the file name, progress report keeps its position when scrolled up; `Total deletions` complete.
  - **Automatic tests**: `python run_tests.py` (Rust, Python, interface and end-to-end tests).
  - **Documentation**: README checked against the application, internal database metrics and sunbursts, snapshots of the component diagrams.
  - Details of all these changes: [documentation/CHANGES_2.0.0.md](documentation/CHANGES_2.0.0.md).

- **31_05_2026**:
  - modernizing GUI and desktop integration with Nuxt 4 + electron.js 

- **01_10_2025**:
  - fixing missing headers issue when writing a previously not existing csv file.
  - optimizing peaks filter with Numba just-in-time (jit)


- **26_09_2025**:
  - remove peaks with intensity <= 0.
  - bugfix with json update

- **15_09_2025**:
  - replace UNKNOWN by NOT FOUND for missing value
  - improve JSON output with pretty humane-readable JSON.
  - adding De Novo fragments formula calculations.
  - adding safety checks to the GUI start button.
  - adding file hash to spectrums (SHA-256 of the file size).
  - disabling automatic instrument deduction (e.g., inferring LC if ESI is found).

- **05_06_2025**:
  - replace RETENTIONTIME default field by RT.
  - Return to the main window when FINISH is pressed.
  - Correction of spectra moved to in-silico if FragHub output is used as input.
  - Deduplicate now on SPLASH **and** INCHIKEY. spectra without INCHIKEY are not deleted at the beginning of the process.

- **14_05_2025**:
  - fix adduct in silico correction

- **18_04_2025**:
  - fix adduct regex pattern
  - improve spectrum deletion callback by writing deleted spectrum in DELETION_REASONS sub folder with a detailed reason.
  - extend precursormz and adduct checks exception to all GC spectrums.
  - modifiy adduct ionmode check with "pos", "neg" in adduct dico.
  - direct integration of spectra-hash (https://github.com/berlinguyinca/spectra-hash) into fraghub.
  - Correcting and adduct some adducts to adduct dict
  - Adding loading screen to GUI
  - adding new gc instruments to instrument dict.
  - auto add [M+H]+ or [M-H]- in In-Silico if adduct is missing.
  - creating FragHub executable for Windows, Linux, and macOS with Python fully integrated.


- **03_04_2025**:
  - fixing missing last msp spectrum in msp files in some cases.


- **25_03_2025**:
  - fixing peaks list regex parsing error when formulas in peaks comment.
  

- **29_01_2025**:
  - changing licence from MIT to CC-BY-NC 4.0


- **27_01_2025**:
  - Refactoring updates and project logic.


- **24_01_2025**: 
  - deleting spectrum if neg adduct in pos spectrum, or pos adduct in neg spectrum
  - bug(fix): let thread safely terminate in no new spectrum to process
  - removing no or bad adduct spectrum
  - refactoring adduct normalization
  - adding report.txt in output directory


- **20_12_2024**: 
  - New modern GUI (Graphical User Interface).
  - Adding dependencies to requirements.txt
  - Removing MacOS and Linux support 😢 (this is just good-bye).
  - Refactoring duplicatas removal, now by sames SPLASH key.
  - Refactoring filtering logic with GC case.
  - Adding logs to GUI for monitoring suppressed spectrum.
  - Now completing NAMES and descriptor from RDkit and PubChem datas (offline).
  - Now completing Classyfire and NPclassifier from local datas.
  - Auto calculating chunk size for multi threading pool.
  - Now deleting spectrum with no SMILES no InChI **AND no inchikey**.
  - Refactoring .json reader for standard ISO/IEC 20802-2:2016 .json **and non-standard formats**.
  - Moving all globals variables to a single file.