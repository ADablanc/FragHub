; Ajouts de FragHub au script de l'installeur Windows. electron-builder lit ce fichier tout seul
; (option nsis.include, fichier par défaut build/installer.nsh). Les modèles NSIS d'electron-builder sont aussi
; modifiés à chaque build par build_all.py (patch_nsis_templates).

; Dossier de l'application préparé par electron-builder : ses fichiers sont copiés un par un par NSIS
!define FRAGHUB_APP_DIR "${PROJECT_DIR}\dist_electron\win-unpacked"

