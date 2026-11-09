# Chantier collision énergie

Normalisation de l'énergie de collision (COLLISIONENERGY) et de la méthode de fragmentation
(FRAGMENTATIONMETHODE), et inventaire des champs qui les portent dans les sources.
- Étape 1 (en cours) : les lecteurs récupèrent-ils TOUTES les énergies des sources, où qu'elles soient ?
- Étape 2 (plus tard) : normaliser les valeurs (l'ancien code Python n'est pas à reprendre tel quel).

Format d'une entrée : date — source(s) — constat — exemple(s) — piste.

## Étape 1 — audit des lecteurs (2026-11-09, PC, `scratch/formats_test/ce_audit.py`, rapport
`OUT_INFO/energies_collision.txt`)

Spectres associés à la sortie avec une énergie dans le source mais COLLISIONENERGY vide en sortie :

| Source | associés | perdus | cause |
|---|---|---|---|
| MSnLib 2024 (16 fichiers) | non associables | — | sortie : 0 % vide, mêmes formes que le source → rien de perdu |
| MSnLib 2025 / targetmol | 12 030 | 0 | |
| GNPS JSON / MGF / MSP | ~15 200 | 12 / 14 / 12 | énergie dans le nom « TYRAMINE - 20.0 eV », « … - 50.0 eV M+H », « …20eV M-H », « (10-30V+20V) » non lue |
| MS-DIAL VS19 neg / pos | 2 095 / 4 382 | 0 / 0 | |
| MassBank NIST | 7 360 | 0 | |
| MassBank RIKEN | 7 391 | 14 | format sans énergie (limite d'export) ; parfois « at 30.0 NCE » dans le commentaire |
| Metaboanalyst NEG / POS | 24 279 / 20 043 | 2 / 24 | identifiants donnés à deux spectres (pas une perte) + noms GNPS « - 30.0 eV » |
| Metaboanalyst prédits | ~25 000 | 0 | |
| MoNA JSON / MSP | ~19 590 | 8 / 10 | champs non reliés (ci-dessous), « CE 20V » dans le nom, DNAAdduct à examiner |
| spectraverse | 19 513 | 3 | « …; AIF; CE0; MS2Dec » (CE0 volontairement ignoré) |

Pistes de correction (étape 1), à valider :
- GNPS : lire « - <n> eV » (suivi éventuellement d'un adduit) en fin de nom, comme « CollisionEnergy:… » déjà lu.
- MoNA : relier `collision energy voltage`, `stepped nce`, `hcd` (« 30 NCE »), `collision energies` (Enveda) à
  COLLISIONENERGY.
- MS-DIAL : « CE 20V » (avec espace et unité) en fin de nom.
- MoNA DNAAdduct (`collisiontype=hcd`, `collisionenergy=90` dans les commentaires) : énergie perdue, à examiner.
- MassBank RIKEN : « merged repeated ion fragments at <n> NCE/eV » dans le commentaire (motif précis) : optionnel.

Pas des énergies de collision (ne pas relier) : `collision energy spread` (« 15 V », ~13 % de MoNA : écart
autour de l'énergie nominale), `collision gas` (N2, He, Ar), `ionization energy` EI (70 eV), tensions de
cône / capillaire / source / spray / lentilles, « 3 kV » dans le champ énergie de MassBank.

À décider : `collision energy level` (« low », « med », « high », HMDB via MoNA) ; spectraverse
`normalized_collision_energy_1/2/3` (NCE en plus de l'énergie absolue déjà lue) ; énergies dans le nom du
fichier d'acquisition GNPS (`source_file` « negativo 35eV ») : peu fiable, à ignorer.

## Étape 2 — matière pour la normalisation (formes relevées)
- Nombres seuls (« 40 », « 35.0 »), avec unité (« 40 V », « 20.0V », « 35 eV », « 25eV »), négatifs (« -20 »,
  « -60,-40,-20 » Enveda, « -10, -35, -60V »).
- Listes : « 20.0, 30.0, 60.0 », « [20.0, 30.0, 60.0] » (MSnLib), « 15,30,45 (NCE) », « 15, 30, 45, 60, 70 or
  90 (nominal) », « 0.60/0.55 » (Keio), « 15 / 50 », « 10 20 » (GNPS MGF).
- Concaténé : « CollisionEnergy:102040 » (GNPS = 10/20/40).
- Pourcentages / NCE : « 35 % (nominal) », « 30% », « 35  (nominal) », « 20(NCE) », « 30.0 NCE », « NCE=80% ».
- Rampes : « Ramp 5-60 V », « RAMP 20-30 eV », « Ramp 20%-70% (nominal) », « 10->20V », « 30->35% », « 10-35,ramp »,
  « Ramped collision energies of 20 to 40 V », « scaled 80 (m/z=200) to 110 (m/z=120) », « scaled by m/z ».
- Avec la méthode : « 65HCD », « HCD (NCE 20-30-40%) », « HCD (NCE 40%) », « 35 CID », « hcd30.00 », « 120 HCD ».
- Avec l'analyseur : « 35 eV FT-MS », « 35 eV IT-MS ».
- Texte : « low », « med », « high ».
- spectraverse : énergie absolue (« 11.326204 ») et NCE (« 20.0 ») séparées.

### Méthode de fragmentation (FRAGMENTATIONMETHODE)
- 2026-11-09 — toutes sources — FRAGMENTATIONMETHODE vide à ~100 % en sortie, alors que l'information existe,
  mêlée à l'énergie (« HCD (NCE 20-30-40%) », « 35 CID », « HCD 90 », « 65HCD »). Piste : extraire la méthode du
  champ énergie avant de normaliser l'énergie.
- 2026-11-09 — MS-DIAL VS19, MoNA — méthode dans l'origine / la description de bibliothèque :
  « origin=HCD natural product library », « UVPD on Thermo IQX ». Piste : à n'utiliser que si rien d'autre.
- 2026-11-09 — MoNA — `fragmentation mode`, `fragmentation method`, `collisiontype` (« HCD », « cid ») reliés à
  FRAGMENTATIONMETHODE ; aucune normalisation aujourd'hui (« cid » / « CID », listes, « Q » de
  `fragmentation equipment`). MoNA `method` (« HCD », 54 sur 20 000) : nom trop général.

## Fait / expliqué
- 2026-11-09 — Metaboanalyst CSV — énergies « perdues » : identifiants donnés à deux spectres différents dans le
  source (le script compare parfois le mauvais), pas une perte de FragHub.
- 2026-11-09 — MS-DIAL « Nom; CE30; InChIKey » : déjà lu (normalize_name.rs) ; « CE0 » volontairement ignoré.
- 2026-11-09 — GNPS « CollisionEnergy:102040 » dans le nom : déjà lu (tous formats).
