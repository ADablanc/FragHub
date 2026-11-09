# Chantier collision énergie

Chantier reporté : normalisation de l'énergie de collision (COLLISIONENERGY) et de la méthode de fragmentation
(FRAGMENTATIONMETHODE), et inventaire des champs qui les portent dans les sources.
On note ici, au fil de l'eau, ce qu'on remarque et qu'on fera plus tard.

Format d'une entrée : date — source(s) — constat — exemple(s) — piste.

## À faire

### Méthode de fragmentation (FRAGMENTATIONMETHODE)
- 2026-11-09 — toutes sources — FRAGMENTATIONMETHODE est vide à ~100 % en sortie, alors que l'information existe,
  mêlée à l'énergie de collision : « HCD (NCE 20-30-40%) », « 35 CID », « HCD 90 », « CID 45 », « 120 HCD »
  (MS-DIAL VS19, MassBank NIST, MoNA). Piste : extraire la méthode du champ énergie avant de normaliser l'énergie.
- 2026-11-09 — MS-DIAL VS19, MoNA — méthode dans l'origine / la description de bibliothèque :
  « origin=HCD natural product library », « UVPD on Thermo IQX ». Piste : à n'utiliser que si rien d'autre.
- 2026-11-09 — MoNA — `fragmentation mode`, `fragmentation method`, `collisiontype` (« HCD », « cid ») reliés à
  FRAGMENTATIONMETHODE (correspondances ajoutées hors chantier, 2026-11-09). FRAGMENTATIONMETHODE n'a aucune
  normalisation aujourd'hui : décider d'une écriture unique (« cid » / « CID », listes, « Q » de
  `fragmentation equipment`).
- 2026-11-09 — MoNA — champ `method` (« HCD », 54 sur 20 000) : nom trop général pour être relié sans regarder
  ses autres valeurs.

### Énergie de collision (COLLISIONENERGY)
- 2026-11-09 — GNPS (JSON, MGF, MSP), Metaboanalyst — énergie écrite dans le nom :
  « LUMICHROME - 60.0 eV », « LIPOAMIDE - 50.0 eV M+H », « CORTISOL - 30.0 eV ». ~0,1 % des spectres GNPS.
  Piste : extraire « - <n> eV » en fin de nom (et nettoyer le nom ?).
- 2026-11-09 — MS-DIAL (via MoNA, spectraverse) — notation « CE0 », « CE30 » dans le nom
  (« Cyanocobalamin; AIF; CE0; MS2Dec »). Attention aux faux amis : « CE 14:1 » = ester de cholestérol.
- 2026-11-09 — MoNA — champs inconnus de key_to_convert : `collision energy voltage` (« 10 », avec espaces ;
  seule la forme `collision-energy-voltage` est reliée), `collision energy spread` (« 15 V », 2 707 sur 20 000),
  `normalized collion energy` (faute de frappe, « 35.0 »), `collision energy level` (« low »), `stepped nce`
  (« 10, 30, 50 »). À relier à COLLISIONENERGY une fois le format des valeurs décidé.
- 2026-11-09 — spectraverse — `normalized_collision_energy_1/2/3` (« 20.0 », « nan ») non lus ; seuls
  `collision_energy_1/2/3` le sont (réunis).
- 2026-11-09 — MoNA — `ionization energy` / `ionization potential` « 70 eV » : énergie des électrons en EI,
  PAS une énergie de collision. Décider si on la garde quelque part ou si on l'ignore.
- 2026-11-09 — MassBank RIKEN — pas d'énergie dans l'en-tête ; parfois dans le commentaire
  (« merged repeated ion fragments at 30.0 NCE ») : marginal (14 sur 7 391).
- 2026-11-09 — GNPS — énergie dans le nom du fichier d'acquisition (`source_file` « negativo 35eV »,
  « …_NEG_MS2_20-40eV.mgf ») : peu fiable, probablement à ignorer.

## Fait / expliqué
- 2026-11-09 — Metaboanalyst CSV — COLLISIONENERGY vide alors que la colonne `collisionenergy` vaut « 15 »,
  « 45 »… : pas un problème d'énergie. Même identifiant pour deux spectres du fichier (doublons) : le doublon
  gardé n'avait pas l'énergie (voir la suppression des doublons, hors chantier).
