// Nom de la base d'origine d'un fichier d'entrée, deviné d'après le nom du fichier (onglet INPUT).
// Ce n'est qu'une proposition : le nom reste modifiable dans la liste, et un nom non reconnu donne « Unknown ».

/** Noms proposés dans la liste de l'onglet INPUT */
export const DATABASE_NAMES = ['MoNA', 'GNPS', 'MassBank', 'MSnLib', 'MS-DIAL', 'HMDB', 'NIST', 'Respect', 'Spectraverse']

// Un mot doit commencer au début du nom ou après un caractère qui n'est pas une lettre (« ALL_GNPS », « MoNA-export »),
// pour ne pas reconnaître « mona » dans « monacolin ». Les règles sont essayées dans l'ordre : la première gagne.
const debut = '(?:^|[^a-z])'
const fin = '(?:[^a-z]|$)'
const RULES = [
  // Bibliothèques MSnLib (Zenodo) : un fichier par collection de molécules, « 20241003_enamdisc_pos_ms2.mgf »
  ['MSnLib', new RegExp(debut + '(?:msnlib|enamdisc|enammol|mcebio|mcedrug|mcescaf|mcediv|nihnp|otavapep|targetmol)')],
  // Bibliothèques publiques de MS-DIAL : « MSMS-Public_experimentspectra-pos-VS19.msp »
  ['MS-DIAL', new RegExp(debut + '(?:msms-public|msdial|ms-dial)')],
  ['Spectraverse', new RegExp(debut + 'spectraverse')],
  ['MoNA', new RegExp(debut + 'mona' + fin)],
  ['GNPS', new RegExp(debut + '(?:gnps|ccmslib)')],
  ['MassBank', new RegExp(debut + '(?:massbank|msbnk)')],
  ['HMDB', new RegExp(debut + 'hmdb')],
  ['NIST', new RegExp(debut + 'nist')],
  ['Respect', new RegExp(debut + 'respect' + fin)],
]

/** Base d'origine devinée d'après le nom (ou le chemin complet) du fichier, « Unknown » si rien n'est reconnu */
export function guessDatabaseName(path) {
  const name = String(path ?? '').split(/[/\\]/).pop().toLowerCase()
  for (const [database, pattern] of RULES) {
    if (pattern.test(name)) return database
  }
  return 'Unknown'
}
