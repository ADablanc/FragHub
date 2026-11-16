// Tests des onglets de réglage : chaque interrupteur et chaque champ modifie le bon réglage, celui qui est envoyé au
// serveur Python au lancement (POST /run-analysis).
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import InputTab from '~/components/InputTab.vue'
import OutputTab from '~/components/OutputTab.vue'
import FiltersTab from '~/components/FiltersTab.vue'
import DeNovoTab from '~/components/DeNovoTab.vue'
import OutputSettingTab from '~/components/OutputSettingTab.vue'
import ProjectsTab from '~/components/ProjectsTab.vue'
import { useState } from '#imports'
import { DEFAULTS, resetParameters, emptyParameters } from './outils.js'

let parameters
beforeEach(() => {
  parameters = resetParameters()
  useState('invalidFields', () => ({})).value = {}
})
afterEach(() => { delete window.electronAPI })

/** Fenêtres de choix d'Electron (electron/preload.cjs), remplacées par des réponses connues */
function fakeElectron({ files = [], folder = null } = {}) {
  window.electronAPI = { selectFiles: async () => files, selectFolder: async () => folder }
}

const switches = (w) => w.findAll('input[type="checkbox"]')
const textFields = (w) => w.findAll('input[type="text"]')

// ---------------------------------------------------------------------------------------------------------------
describe('Onglet INPUT', () => {
  it('sans fichier : invite à choisir des fichiers, pas de liste', async () => {
    const w = await mountSuspended(InputTab)
    expect(w.text()).toContain('Select input files')
    expect(w.text()).not.toContain('Selected Files')
  })

  it('fichiers choisis dans Electron : chemins complets ajoutés, base devinée selon le nom, noms affichés', async () => {
    fakeElectron({ files: ['C:\\data\\MoNA.msp', '/home/axel/GNPS.mgf'] })
    const w = await mountSuspended(InputTab)
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(parameters.value.input_directory).toEqual(['C:\\data\\MoNA.msp', '/home/axel/GNPS.mgf'])
    expect(parameters.value.input_db_names).toEqual({ 'C:\\data\\MoNA.msp': 'MoNA', '/home/axel/GNPS.mgf': 'GNPS' })
    // Seul le nom du fichier est affiché, que le chemin soit écrit avec « \ » ou « / »
    expect(w.text()).toContain('Selected Files (2)')
    expect(w.text()).toContain('MoNA.msp')
    expect(w.text()).toContain('GNPS.mgf')
    expect(w.text()).not.toContain('C:\\data')
    expect(w.text()).toContain('Add more files')
  })

  it('deuxième choix : fichiers ajoutés à la suite, nom de base déjà choisi gardé', async () => {
    resetParameters({ ...DEFAULTS, input_directory: ['/d/a.msp'], input_db_names: { '/d/a.msp': 'MoNA' } })
    fakeElectron({ files: ['/d/b.json'] })
    const w = await mountSuspended(InputTab)
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    const p = (await import('#imports')).useState('parameters').value
    expect(p.input_directory).toEqual(['/d/a.msp', '/d/b.json'])
    expect(p.input_db_names).toEqual({ '/d/a.msp': 'MoNA', '/d/b.json': 'Unknown' })
  })

  it('fichier déjà choisi : pas ajouté une deuxième fois, son nom de base est gardé', async () => {
    const p = resetParameters({ ...DEFAULTS, input_directory: ['/d/a.msp'], input_db_names: { '/d/a.msp': 'MoNA' } })
    fakeElectron({ files: ['/d/a.msp', '/d/b.mgf', '/d/b.mgf'] })
    const w = await mountSuspended(InputTab)
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(p.value.input_directory).toEqual(['/d/a.msp', '/d/b.mgf'])
    expect(p.value.input_db_names).toEqual({ '/d/a.msp': 'MoNA', '/d/b.mgf': 'Unknown' })
    expect(w.text()).toContain('Selected Files (2)')
    // Rien de nouveau : liste inchangée
    fakeElectron({ files: ['/d/b.mgf'] })
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(p.value.input_directory).toEqual(['/d/a.msp', '/d/b.mgf'])
  })

  it('choix annulé : rien ne change', async () => {
    fakeElectron({ files: [] })
    const w = await mountSuspended(InputTab)
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(parameters.value.input_directory).toEqual([])
  })

  it('suppression d\'un fichier puis de tous', async () => {
    const p = resetParameters({ ...DEFAULTS, input_directory: ['/d/a.msp', '/d/b.mgf', '/d/c.csv'],
                                input_db_names: { '/d/a.msp': 'MoNA', '/d/b.mgf': 'GNPS', '/d/c.csv': 'Unknown' } })
    const w = await mountSuspended(InputTab)
    // Boutons « corbeille » de chaque ligne (après le bouton de choix et « Clear all »)
    const deleteButtons = w.findAll('button.v-btn--icon').filter((b) => b.html().includes('mdi-delete') && !b.text())
    expect(deleteButtons.length).toBe(3)
    await deleteButtons[1].trigger('click')
    expect(p.value.input_directory).toEqual(['/d/a.msp', '/d/c.csv'])
    expect(p.value.input_db_names).toEqual({ '/d/a.msp': 'MoNA', '/d/c.csv': 'Unknown' })
    const clearAll = w.findAll('button').find((b) => b.text().includes('Clear all'))
    await clearAll.trigger('click')
    expect(p.value.input_directory).toEqual([])
    expect(p.value.input_db_names).toEqual({})
    expect(w.text()).toContain('Select input files')
  })

  it('nom de la base saisi pour un fichier : enregistré pour ce fichier', async () => {
    const p = resetParameters({ ...DEFAULTS, input_directory: ['/d/a.msp'], input_db_names: { '/d/a.msp': 'Unknown' } })
    const w = await mountSuspended(InputTab)
    await w.find('.v-combobox input').setValue('MaBase')
    expect(p.value.input_db_names['/d/a.msp']).toBe('MaBase')
  })

  it('sélecteur de fichiers : seulement les extensions lues par FragHub', async () => {
    const w = await mountSuspended(InputTab)
    expect(w.find('input[type="file"]').attributes('accept')).toBe('.json,.jsonl,.csv,.msp,.mgf')
  })
})

// ---------------------------------------------------------------------------------------------------------------
describe('Onglet OUTPUT', () => {
  it('dossier choisi dans Electron : chemin complet enregistré et affiché', async () => {
    fakeElectron({ folder: 'C:\\Users\\Axel\\FragHub_sortie' })
    const w = await mountSuspended(OutputTab)
    expect(w.text()).toContain('No directory selected')
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(parameters.value.output_directory).toBe('C:\\Users\\Axel\\FragHub_sortie')
    expect(w.text()).toContain('C:\\Users\\Axel\\FragHub_sortie')
    expect(w.text()).not.toContain('No directory selected')
  })

  it('pas de champ de fichier caché (le dossier est choisi par Electron)', async () => {
    const w = await mountSuspended(OutputTab)
    expect(w.find('input[type="file"]').exists()).toBe(false)
  })

  it('choix annulé : dossier précédent gardé', async () => {
    const p = resetParameters({ ...DEFAULTS, output_directory: '/sortie' })
    fakeElectron({ folder: null })
    const w = await mountSuspended(OutputTab)
    await w.find('button').trigger('click')
    await new Promise((r) => setTimeout(r))
    expect(p.value.output_directory).toBe('/sortie')
  })
})

// ---------------------------------------------------------------------------------------------------------------
const FILTERS = ['normalize_intensity', 'remove_peak_above_precursormz', 'check_minimum_peak_requiered',
  'reduce_peak_list', 'remove_spectrum_under_entropy_score', 'keep_mz_in_range', 'check_minimum_of_high_peaks_requiered']
const FILTER_VALUES = ['check_minimum_peak_requiered_n_peaks', 'reduce_peak_list_max_peaks',
  'remove_spectrum_under_entropy_score_value', 'keep_mz_in_range_from_mz', 'keep_mz_in_range_to_mz',
  'check_minimum_of_high_peaks_requiered_intensity_percent', 'check_minimum_of_high_peaks_requiered_no_peaks']

describe('Onglet Filters', () => {
  it('réglages absents : valeurs par défaut de FragHub posées par l\'onglet', async () => {
    const p = emptyParameters()
    await mountSuspended(FiltersTab)
    for (const key of [...FILTERS, ...FILTER_VALUES]) expect(p.value[key], key).toBe(DEFAULTS[key])
  })

  it('un interrupteur et un champ par filtre, dans l\'ordre, avec les valeurs en cours', async () => {
    const w = await mountSuspended(FiltersTab)
    for (const name of FILTERS) expect(w.text()).toContain(name)
    expect(switches(w).length).toBe(FILTERS.length)
    expect(textFields(w).map((t) => t.element.value)).toEqual(FILTER_VALUES.map((k) => String(DEFAULTS[k])))
  })

  it('chaque interrupteur ne change que son filtre (1 = actif, 0 = inactif)', async () => {
    const w = await mountSuspended(FiltersTab)
    for (const [i, key] of FILTERS.entries()) {
      const before = { ...parameters.value }
      await switches(w)[i].setValue(false)
      expect(parameters.value).toEqual({ ...before, [key]: 0.0 })
      await switches(w)[i].setValue(true)
      expect(parameters.value[key]).toBe(1.0)
    }
  })

  it('chaque champ ne change que sa valeur ; virgule décimale acceptée', async () => {
    const w = await mountSuspended(FiltersTab)
    for (const [i, key] of FILTER_VALUES.entries()) {
      const before = { ...parameters.value }
      await textFields(w)[i].setValue('12,5')
      expect(parameters.value).toEqual({ ...before, [key]: 12.5 })
      await textFields(w)[i].setValue('7')
      expect(parameters.value[key]).toBe(7)
    }
  })

  it('saisie invalide : signalée en rouge, jamais envoyée, réglage gardé à sa dernière valeur valide', async () => {
    const w = await mountSuspended(FiltersTab)
    const invalid = useState('invalidFields')
    const field = () => textFields(w)[0]       // check_minimum_peak_requiered_n_peaks
    for (const text of ['', 'abc', '1,234,5', '-3']) {
      await field().setValue(text)
      expect(parameters.value.check_minimum_peak_requiered_n_peaks, text).toBe(3)
      expect(invalid.value).toEqual({ check_minimum_peak_requiered_n_peaks: true })
      expect(field().element.value).toBe(text)               // le texte saisi reste affiché pour être corrigé
      expect(w.text()).toContain('Number expected')
    }
    await field().setValue('4,5')
    expect(parameters.value.check_minimum_peak_requiered_n_peaks).toBe(4.5)
    expect(invalid.value).toEqual({})
    expect(w.text()).not.toContain('Number expected')
  })

  it('onglet rouvert après une saisie invalide : champs repartis des valeurs valides, plus rien de bloqué', async () => {
    let w = await mountSuspended(FiltersTab)
    await textFields(w)[1].setValue('xx')
    expect(useState('invalidFields').value).toEqual({ reduce_peak_list_max_peaks: true })
    w.unmount()
    w = await mountSuspended(FiltersTab)
    expect(textFields(w)[1].element.value).toBe('500')
    expect(useState('invalidFields').value).toEqual({})
  })
})

// ---------------------------------------------------------------------------------------------------------------
describe('Onglet De Novo', () => {
  it('désactivé par défaut, tolérance de 10 ppm', async () => {
    const p = emptyParameters()
    const w = await mountSuspended(DeNovoTab)
    expect(p.value.calculate_de_novo).toBe(0.0)
    expect(p.value.de_novo_ppm_tolerance).toBe(10.0)
    expect(w.find('input[type="text"]').element.value).toBe('10')
  })

  it('interrupteur et tolérance (virgule décimale acceptée)', async () => {
    const w = await mountSuspended(DeNovoTab)
    await w.find('input[type="checkbox"]').setValue(true)
    expect(parameters.value.calculate_de_novo).toBe(1.0)
    await w.find('input[type="text"]').setValue('2,5')
    expect(parameters.value.de_novo_ppm_tolerance).toBe(2.5)
    await w.find('input[type="checkbox"]').setValue(false)
    expect(parameters.value.calculate_de_novo).toBe(0.0)
  })

  it('tolérance invalide : signalée, la dernière valeur valide est gardée (avant : 0 ppm)', async () => {
    const w = await mountSuspended(DeNovoTab)
    await w.find('input[type="text"]').setValue('abc')
    expect(parameters.value.de_novo_ppm_tolerance).toBe(10.0)
    expect(useState('invalidFields').value).toEqual({ de_novo_ppm_tolerance: true })
    expect(w.text()).toContain('Number expected')
    await w.find('input[type="text"]').setValue('5')
    expect(parameters.value.de_novo_ppm_tolerance).toBe(5)
    expect(useState('invalidFields').value).toEqual({})
  })
})

// ---------------------------------------------------------------------------------------------------------------
describe('Onglet Output settings', () => {
  const FORMATS = { CSV: 'csv', MSP: 'msp', JSON: 'json', 'mzSpecLib (JSON)': 'mzspeclib_json' }

  it('les quatre formats, tous écrits par défaut', async () => {
    const p = emptyParameters()
    const w = await mountSuspended(OutputSettingTab)
    for (const [label, key] of Object.entries(FORMATS)) {
      expect(w.text()).toContain(label)
      expect(p.value[key], key).toBe(1.0)
    }
  })

  it('chaque interrupteur ne change que son format', async () => {
    const w = await mountSuspended(OutputSettingTab)
    for (const [i, key] of Object.values(FORMATS).entries()) {
      const before = { ...parameters.value }
      await switches(w)[i].setValue(false)
      expect(parameters.value).toEqual({ ...before, [key]: 0.0 })
    }
  })
})

// ---------------------------------------------------------------------------------------------------------------
describe('Onglet Projects settings', () => {
  it('remise à zéro impossible tant que le dossier de sortie n\'est pas un dossier FragHub', async () => {
    const p = resetParameters({ ...DEFAULTS, output_directory: '/donnees/sortie', reset_updates: 1.0 })
    const w = await mountSuspended(ProjectsTab)
    // Valeur remise à NO, interrupteur désactivé
    expect(p.value.reset_updates).toBe(0.0)
    expect(switches(w)[0].attributes('disabled')).toBeDefined()
    expect(w.text()).toContain('NO')
  })

  it('dossier FragHub : remise à zéro possible ; un autre dossier choisi ensuite la désactive', async () => {
    const p = resetParameters({ ...DEFAULTS, output_directory: '/donnees/FragHub_sortie' })
    const w = await mountSuspended(ProjectsTab)
    expect(switches(w)[0].attributes('disabled')).toBeUndefined()
    await switches(w)[0].setValue(true)
    expect(p.value.reset_updates).toBe(1.0)
    expect(w.text()).toContain('YES')
    p.value.output_directory = '/autre/dossier'
    await w.vm.$nextTick()
    expect(p.value.reset_updates).toBe(0.0)
  })

  it('journal de performances : désactivé par défaut, interrupteur à part', async () => {
    const p = emptyParameters()
    const w = await mountSuspended(ProjectsTab)
    expect(p.value.write_perf_logs).toBe(0.0)
    expect(w.text()).toContain('Write perf logs')
    await switches(w)[1].setValue(true)
    expect(p.value.write_perf_logs).toBe(1.0)
    expect(p.value.reset_updates).toBe(0.0)
  })
})
