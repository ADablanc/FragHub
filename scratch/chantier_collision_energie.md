# Chantier collision énergie

Normalisation de l'énergie de collision (COLLISIONENERGY) et de la méthode de fragmentation
(FRAGMENTATIONMETHODE), et inventaire des champs qui les portent dans les sources.
(Copie du document du projet `claude/chantier-collision-energie.md`, qui fait foi.)
- Étape 1 (TERMINÉE, 2026-11-09) : les lecteurs récupèrent toutes les énergies des sources (audit final : plus
  aucune vraie perte ; restes expliqués ci-dessous).
- Étape 2 (codée les 2026-11-09 / 10, `normalizer/normalize_collision_energy.rs`, étapes 3b et 15 de
  values_normalizer ; vérifiée sur l'échantillon le 2026-11-10, non commitée) : uniformiser l'écriture, **sans
  conversion d'unité**, et remplir la méthode de fragmentation.

## Étape 2 — règles (décisions d'Axel, 2026-11-09 / 10)

- **Aucune conversion d'unité** : eV reste eV, V reste V, kV reste kV.
- **Décimales inutiles retirées** : `20.0` → `20`, `0.60` → `0.6` ; les vraies décimales restent (`17.6`).
- **Pourcentages, NCE, « nominal » → `NCE`** : `75 % (nominal)`, `75%`, `35  (nominal)`, `30.0 NCE`, `10(NCE)`,
  `NCE=80%` → `75 NCE`, `35 NCE`, `30 NCE`, `10 NCE`, `80 NCE`.
- **Valeur simple** : `nombre espace unité` (`35 eV`, `20 V`, `75 NCE`) ; sans unité : le nombre seul (`40`).
- **Plusieurs énergies** : séparées par `/`, puis l'unité. Mot « Ramp » / « Stepped » seulement quand la source le
  dit ou que l'écriture le rend évident ; sinon liste neutre :
  - `Ramp` : « ramp », « ramped », flèche `->`, « … to … », ou exactement deux valeurs reliées par un tiret
    (`Ramp 5-60 V` → `Ramp 5/60 V`, `10->20V` → `Ramp 10/20 V`, `20-40eV` → `Ramp 20/40 eV`, `30-50` → `Ramp 30/50`) ;
  - `Stepped` : « stepped », ou trois valeurs et plus reliées par des tirets (Thermo : `HCD (NCE 20-30-40%)` →
    `Stepped 20/30/40 NCE`) ;
  - neutre : virgules, barres, espaces, crochets, tirets bas (`[20.0, 30.0, 60.0]` → `20/30/60`, `-10, -35, -60V` →
    `-10/-35/-60 V`, `NCE_20_30_40` → `20/30/40 NCE`) ; GNPS `102040` → `10/20/40` ; `CE75` → `75`.
- **Méthode écrite avec l'énergie** (`35 CID`, `65HCD`, `hcd30.00`, `HCD (NCE …)`) : mise dans
  FRAGMENTATIONMETHODE si vide (en majuscules), retirée de l'énergie.
- **Analyseur écrit après l'énergie** (MassBank KWR `35 eV FT-MS`, `FT-MS II`, `IT-MS`) : retiré de l'énergie
  (`35 eV`), mis dans INSTRUMENTTYPE seulement si ce champ est vide (jamais écrasé ; dans l'échantillon il vaut déjà
  `LC-ESI-ITFT`).
- **Laissés tels quels** : `CE0`, `low` / `med` / `high`, `3 kV` et `Ramp 10-30 kV` (voir plus bas),
  `10-30V+20V` (GNPS), `scaled by m/z`, `scaled 80 (m/z=200) to 110 (m/z=120)`,
  `15, 30, 45, 60, 70 or 90 (nominal)` (« or » : une seule de ces valeurs, pas une liste), MoNA
  `6V (MSn); Ramp 15V-35V (MSe)`, toute écriture non reconnue.

Points encore ouverts :
- spectraverse : rampe = champs 1 et 2, paliers = champs 1, 2, 3 ; aujourd'hui réunis de la même façon → liste
  neutre ; à distinguer à la lecture plus tard. Valeurs en eV d'après l'article, mais écrites sans unité.

## Méthode de fragmentation (FRAGMENTATIONMETHODE) — règles (2026-11-10)

- Sigles reconnus : HCD, CID, CAD, ETD, ECD, EAD, EID, UVPD, IRMPD, PQD, EThcD, ETciD ; « in source CID » →
  `in-source CID`. Jamais une recherche brute : sigle isolé (pas de lettre collée), en majuscules hors des champs de
  méthode, jamais un identifiant PubChem (`CID 638`, `CID:102004382`, `pubchem cid=…`).
- Lue (si le champ est vide) dans : l'énergie, l'instrument et son type (GNPS « HCD; Lumos »), les indices
  d'instrument, l'origine / la bibliothèque (MoNA `library`, `tags`, `origin`), l'origine écrite dans le commentaire
  (MS-DIAL « origin=… », MoNA MSP « "origin=uvpd.msp" »), et le nom GNPS (`Catechin_HCD40_170906`).
- Plusieurs méthodes citées : rien, sauf si une seule commence un nom de bibliothèque ou de fichier
  (« HCD natural product library, acquired in adjunct with UVPD library » → HCD).
- Valeur déjà écrite : un sigle seul passe en majuscules (`cid` → `CID`) ; écrite autrement, gardée telle quelle
  (`LOW-ENERGY CID`) ; **pas une méthode → vidée** (MoNA NIST-Glycan `IT/ion trap`, `IT-FT/ion trap with FTMS`,
  des analyseurs ; décision d'Axel : rien d'autre qu'une méthode dans ce champ).
- Pas de déduction depuis l'instrument (Q Exactive → HCD) : non décidé.
- Laissés : MS3 avec deux méthodes (`…_CID25_MS3-465_HCD100_…`), `CID90; (Acyl-CoA)` (MoNA022285), `source_file`
  GNPS.
- GNPS : les spectres avec la méthode dans l'instrument (« CID; Velos », « HCD; Lumos ») sont presque tous sans
  structure (lipides, « Spectral Match ») et supprimés avant (`no_inchi_smiles_or_inchikey.csv`) ; vide normal.

## Étape 1 — audit des lecteurs (PC, `scratch/formats_test/ce_audit.py`, rapport `OUT_INFO/energies_collision.txt` ;
copie du script dans le projet : `claude/scripts/ce_audit.py`, sans la ligne « PERDUS ex. » ajoutée ensuite ;
audit des méthodes : `scratch/formats_test/fm_audit.py`, rapport `OUT_INFO/methodes_fragmentation.txt`)

| Source | associés | perdus avant | perdus après | reste |
|---|---|---|---|---|
| MSnLib 2024 (16 fichiers) | non associables | — | — | sortie 0 % vide, mêmes formes que le source |
| MSnLib 2025 / targetmol | 12 030 | 0 | 0 | |
| GNPS JSON / MGF / MSP | ~15 200 | 12 / 14 / 12 | 3 / 0 / 0 | JSON : énergie seulement dans `source_file` (ignoré) |
| MS-DIAL VS19 neg / pos | 2 095 / 4 382 | 0 / 0 | 0 / 0 | |
| MassBank NIST | 7 360 | 0 | 0 | |
| MassBank RIKEN | 7 391 | 14 | 0 | |
| Metaboanalyst NEG / POS | 24 279 / 20 043 | 2 / 24 | 2 / 9 | identifiants donnés à deux spectres (pas une perte) |
| Metaboanalyst prédits | ~25 000 | 0 | 0 | |
| MoNA JSON / MSP | ~19 590 | 8 / 10 | 3 / 5 | DNAAdduct (« collision energy = 0 », vide voulu) ; « CID90; (Acyl-CoA) » (ambigu, laissé) ; faux positifs du script |
| spectraverse | 19 513 | 3 | 0 | |

Corrigé le 2026-11-09 (commité) : énergie avec unité en fin de nom (GNPS / Metaboanalyst), morceaux MS-DIAL
« CE 20V » / « 30eV » / « CE0 », énergie dans le commentaire si le champ est vide (MassBank « repeated ion
fragments at 30.0 NCE », « CollisionEnergy:… », « CE:Ramp 5-60 V », « collisionenergy=90 »), champs MoNA
`collision energy voltage` / `level` / `stepped nce` / `hcd` / `collision energies` ; jamais pour un spectre GC / EI.

Décisions : DNAAdduct « collision energy = 0 » laissé vide ; « CID90; (Acyl-CoA) » laissé tel quel.
Pas des énergies de collision : `collision energy spread`, `collision gas`, `ionization energy` EI (70 eV), tensions
de cône / capillaire / source / spray / lentilles. À ignorer : `source_file` GNPS. spectraverse : seulement
`collision_energy_1/2/3` (eV, article PMC12903054), pas la NCE.
**`3 kV` est une vraie énergie de collision** (corrigé le 2026-11-10) : MassBank Kyoto Univ CA…, FAB sur JEOL
JMS-HX110 à secteurs magnétiques (`FAB-EBEB`), collision à quelques keV ; gardé. `Ramp 10-30 kV`
(MSBNK-NILU-NL0484, Orbitrap Exploris) : sans doute une faute de la source, laissé tel quel (on ne devine pas).
