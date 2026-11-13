# Jeu d'entrée du test de bout en bout

Utilisé par `tests/test_bout_en_bout.py` : tout FragHub tourne sur le dossier `entree/`, et chaque fichier de sortie
est comparé octet pour octet à `reference/` (sortie d'un run validé, avec la version de la base interne indiquée dans
`reference/_version_de_la_base.txt`).

## `entree/`

- `exemples/` : quelques spectres de chaque fichier source, pris à intervalles réguliers (5 par fichier), plus un
  spectre in silico par source quand il y en a. Toutes les sources et tous les formats lus par FragHub y sont :
  MoNA (MSP et JSON), GNPS (JSON, MGF, MSP), MassBank (MSP aux formats NIST et RIKEN), MSMS-Public (MS-DIAL),
  MetaboAnalyst (CSV, expérimental et prédit), MSnLib (MGF, JSON, JSONL), Spectraverse (MGF).
  Un spectre par fichier ; le nom du fichier d'origine est gardé devant `__exN`.
- `aleatoire/` : 10 spectres tirés au hasard dans chaque fichier d'origine (outil `scripts/build_dataset`, graine 42),
  recopiés octet pour octet ; `_sample_info.txt` dit d'où ils viennent (ignoré par FragHub).
- `MoNA-export-Experimental_Spectra__acyl_coa.msp` : les quatre Acyl-CoA de MoNA dont l'InChI était perdue à la lecture
  (indicateur `"inchi=1"` des Comments ; MoNA022203, MoNA022204, MoNA022998, MoNA023010).

Les spectres viennent de bibliothèques publiques ; chacun garde la licence de sa source (MoNA, GNPS, MassBank,
MSMS-Public / RIKEN, MetaboAnalyst, MSnLib, Spectraverse), indiquée dans le spectre quand la source la donne. Ils ne
servent qu'aux tests.

Les fichiers de `tests/data/` ne sont jamais convertis par git (`.gitattributes` : `-text`) : la taille d'un fichier
d'entrée entre dans son empreinte (FILEHASH), et une conversion des fins de ligne changerait la sortie.

## Changer le jeu d'entrée ou la référence

Après une modification voulue d'une sortie, ou une nouvelle version de la base :

```
python -m pytest tests/test_bout_en_bout.py --update-reference
git diff --stat tests/data/bout_en_bout/reference
```

Vérifier les différences, puis commiter la nouvelle référence avec la modification qui l'explique.
