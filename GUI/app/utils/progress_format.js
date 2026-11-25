// Textes des barres de progression (écran d'avancement). Même présentation que la ligne de commande
// (scripts/progress_format.py).
//
// Unité envoyée par le moteur avec chaque tâche (item_type) :
// - 'bytes' : lecture d'un fichier ; avancement et total en octets, affichés en MB ou GB, vitesse en MB/s ou GB/s ;
// - étapes d'écriture : avancement en spectres (pourcentage et temps restant exacts), avec les octets écrits à côté
//   (champ `bytes` du message), affichés en MB ou GB, vitesse en MB/s ou GB/s ;
// - autre chose ('spectra', 'molecules'...) : nombre d'éléments et éléments par seconde.
// Unités décimales : 1 MB = 1 000 000 octets, 1 GB = 1 000 000 000 octets (comme les tailles de Zenodo).

const MB = 1e6
const GB = 1e9

/** Unité d'affichage d'une taille : GB à partir de 1 GB, MB sinon */
export function sizeUnit(totalBytes) {
  return totalBytes >= GB ? 'GB' : 'MB'
}

const number = (x, decimals) => x.toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals })

/** Taille dans l'unité donnée (choisie d'après la taille si absente) : « 350.2 MB », « 1.42 GB » */
export function formatSize(bytes, unit = sizeUnit(bytes)) {
  return unit === 'GB' ? `${number(bytes / GB, 2)} GB` : `${number(bytes / MB, 1)} MB`
}

/** Débit : « 120.4 MB/s », « 1.35 GB/s » */
export function formatRate(bytesPerSecond) {
  return `${formatSize(bytesPerSecond)}/s`
}

/**
 * Textes d'une barre : { counts, rate }. `current` / `total` dans l'unité de la tâche, `elapsedSec` en secondes,
 * `writtenBytes` : octets écrits (étapes d'écriture), sinon null.
 */
export function progressTexts({ current, total, itemType, elapsedSec, writtenBytes = null }) {
  const perSecond = (x) => (elapsedSec > 0 ? x / elapsedSec : 0)
  if (itemType === 'bytes') {
    const unit = sizeUnit(total)
    const done = formatSize(current, unit).slice(0, -3)
    return { counts: `${done} of ${formatSize(total, unit)}`, rate: formatRate(perSecond(current)) }
  }
  const counts = `${current} of ${total} ${itemType}`
  if (writtenBytes !== null && writtenBytes !== undefined) {
    return { counts: `${counts} | ${formatSize(writtenBytes)} written`, rate: formatRate(perSecond(writtenBytes)) }
  }
  return { counts, rate: `${perSecond(current).toFixed(2)} ${itemType}/s` }
}
