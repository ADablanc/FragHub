// Tests du nom de base deviné d'après le nom du fichier d'entrée (app/utils/database_name.js).
import { describe, it, expect } from 'vitest'
import { guessDatabaseName, DATABASE_NAMES } from '~/utils/database_name.js'

describe('guessDatabaseName', () => {
  it('reconnaît les fichiers habituels des bases publiques', () => {
    const attendus = {
      '20241003_enamdisc_neg_ms2.mgf': 'MSnLib',
      '20241003_enammol_pos_ms2.mgf': 'MSnLib',
      '20241003_mcebio_neg_ms2.mgf': 'MSnLib',
      '20241003_mcedrug_pos_ms2.mgf': 'MSnLib',
      '20241003_mcescaf_neg_ms2.mgf': 'MSnLib',
      '20241003_nihnp_pos_ms2.mgf': 'MSnLib',
      '20241003_otavapep_neg_ms2.mgf': 'MSnLib',
      '20250828_mcediv_50k_sub_neg_ms2.mgf': 'MSnLib',
      '20250828_targetmol_hts_np_pos_ms2.mgf': 'MSnLib',
      'ALL_GNPS.json': 'GNPS',
      'MassBank_RIKENformat.msp': 'MassBank',
      'MoNA-export-Experimental_Spectra.msp': 'MoNA',
      'MSMS-Public_experimentspectra-neg-VS19.msp': 'MS-DIAL',
      'MSMS-Public_experimentspectra-pos-VS19.msp': 'MS-DIAL',
      'spectraverse-1.0.2.mgf': 'Spectraverse',
      'HMDB_experimental_msms.csv': 'HMDB',
      'nist23_hr_msms.msp': 'NIST',
      'ReSpect_all.msp': 'Respect',
      'MSBNK-RIKEN-PR100001.txt.msp': 'MassBank',
      'CCMSLIB00000001547.mgf': 'GNPS',
    }
    for (const [fichier, base] of Object.entries(attendus)) expect(guessDatabaseName(fichier), fichier).toBe(base)
  })

  it('ne regarde que le nom du fichier, pas les dossiers, quel que soit le séparateur', () => {
    expect(guessDatabaseName('C:\\GNPS\\data\\MoNA.msp')).toBe('MoNA')
    expect(guessDatabaseName('/home/axel/MoNA/mes_spectres.mgf')).toBe('Unknown')
  })

  it('nom non reconnu ou mot pris dans un autre mot : « Unknown »', () => {
    expect(guessDatabaseName('mes_spectres.msp')).toBe('Unknown')
    expect(guessDatabaseName('monacolin_standards.mgf')).toBe('Unknown')
    expect(guessDatabaseName('irrespective.msp')).toBe('Unknown')
    expect(guessDatabaseName('')).toBe('Unknown')
    expect(guessDatabaseName(undefined)).toBe('Unknown')
  })

  it('chaque base devinée figure dans la liste proposée', () => {
    for (const f of ['x_enamdisc.mgf', 'msms-public.msp', 'spectraverse.mgf', 'mona.msp', 'gnps.mgf', 'massbank.msp',
      'hmdb.csv', 'nist.msp', 'respect.msp']) {
      expect(DATABASE_NAMES).toContain(guessDatabaseName(f))
    }
  })
})
