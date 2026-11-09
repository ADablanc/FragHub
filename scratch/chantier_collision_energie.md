# Chantier collision énergie

Normalisation de l'énergie de collision (COLLISIONENERGY) et de la méthode de fragmentation
(FRAGMENTATIONMETHODE), et inventaire des champs qui les portent dans les sources.
- Étape 1 (TERMINÉE, 2026-11-09) : les lecteurs récupèrent toutes les énergies des sources (audit final : plus
  aucune vraie perte ; restes expliqués ci-dessous).
- Étape 2 (ensuite) : **normaliser la typographie, sans aucune conversion d'unité** (ni V → eV, ni NCE → eV ;
  décision d'Axel, 2026-11-09). Modèle : l'écriture de **MassBank** (COLLISION_ENERGY), de préférence.
  - valeur simple : « nombre espace unité » quand il y a une unité (« 20 eV », « 35 % (nominal) » à la MassBank),
    le nombre seul sinon ;
  - plusieurs valeurs (paliers) ou rampe : les nombres puis « espace unité », dans une écriture unique à fixer
    d'après MassBank (« 15, 30, 45 eV », « Ramp 5-60 V »...) ;
  - « CE0 » reste écrit « CE0 » (décision d'Axel).

(Copie du document du projet `claude/chantier-collision-energie.md`, qui fait foi.)

## Étape 1 — audit des lecteurs (`scratch/formats_test/ce_audit.py`, rapport `OUT_INFO/energies_collision.txt`)

| Source | associés | perdus avant | perdus après | reste |
|---|---|---|---|---|
| MSnLib 2024 (16 fichiers) | non associables | — | — | sortie 0 % vide, mêmes formes que le source |
| MSnLib 2025 / targetmol | 12 030 | 0 | 0 | |
| GNPS JSON / MGF / MSP | ~15 200 | 12 / 14 / 12 | 3 / 0 / 0 | JSON : énergie seulement dans `source_file` (ignoré) |
| MS-DIAL VS19 neg / pos | 2 095 / 4 382 | 0 / 0 | 0 / 0 | |
| MassBank NIST | 7 360 | 0 | 0 | |
| MassBank RIKEN | 7 391 | 14 | 0 | |
| Metaboanalyst NEG / POS | 24 279 / 20 043 | 2 / 24 | 2 / 9 | identifiants donnés à deux spectres (pas une perte) ; CE0 (corrigé ensuite) |
| Metaboanalyst prédits | ~25 000 | 0 | 0 | |
| MoNA JSON / MSP | ~19 590 | 8 / 10 | 3 / 5 | CE0 (corrigé ensuite) ; DNAAdduct (« collision energy = 0 », vide voulu) ; « CID90; (Acyl-CoA) » (ambigu, laissé) ; faux positifs du script |
| spectraverse | 19 513 | 3 | 0 | |

Corrigé le 2026-11-09 (commité) :
- énergie avec unité en fin de nom (GNPS / Metaboanalyst : « TYRAMINE - 20.0 eV », « … 20eV M-H »,
  « Asperterrol(10-30V+20V) », « … 40 NCE »), retirée du nom (adduit gardé) ; pas pour un spectre GC / EI ;
- morceaux MS-DIAL « CE 20V », « 30eV » (aussi au milieu du nom, alors le nom reste tel quel) ; « CE0 » repris
  tel quel (« CE0 ») ;
- énergie dans le commentaire si le champ est vide : MassBank « averaging / merged repeated ion fragments at
  30.0 NCE », « CollisionEnergy:102040 », « CE:Ramp 5-60 V », « collisionenergy=90 » ;
- MoNA : `collision energy voltage`, `collision energy level` (gardé), `stepped nce`, `hcd`, `collision energies` ;
  une valeur chiffrée passe devant un niveau en mots.

Décisions :
- DNAAdduct (MoNA) : « collision energy = 0 » avec HCD / CID : valeur de remplissage, laissée vide.
- « CID90; (Acyl-CoA); [M+H]+ » (MoNA022285) : ambigu, laissé tel quel.
- Pas des énergies de collision : `collision energy spread`, `collision gas`, `ionization energy` EI (70 eV),
  tensions de cône / capillaire / source / spray / lentilles, « 3 kV ». À ignorer : `source_file` GNPS.
- spectraverse : seulement `collision_energy_1/2/3` (eV) ; la NCE n'est pas reprise.

## Étape 2 — typographie à la MassBank (sans conversion d'unité)

À faire d'abord : relever les écritures recommandées par le format MassBank (`AC$MASS_SPECTROMETRY:
COLLISION_ENERGY`) et celles présentes dans MassBank (52 formes dans MassBank NIST), puis fixer une écriture cible.

Points d'attention : spectraverse rampe (champs 1-2) / paliers (1-3) aujourd'hui réunis pareil ; « 102040 » GNPS =
10/20/40 collés ; énergie avec la méthode (« 65HCD », « HCD (NCE 20-30-40%) », « 35 CID ») : méthode vers
FRAGMENTATIONMETHODE ; formes relevées : nombres seuls, avec unité, négatifs, listes, pourcentages / NCE, rampes,
avec l'analyseur (« 35 eV FT-MS »), texte (« low », « med », « high »), « CE0 ».

### Méthode de fragmentation (FRAGMENTATIONMETHODE)
- FRAGMENTATIONMETHODE vide à ~100 % alors que l'information existe dans l'énergie (« HCD (NCE 20-30-40%) »,
  « 35 CID », « 65HCD ») ou l'origine (« HCD natural product library », « UVPD on Thermo IQX »).
- MoNA `fragmentation mode`, `fragmentation method`, `collisiontype` reliés ; aucune normalisation (« cid » / « CID »).
