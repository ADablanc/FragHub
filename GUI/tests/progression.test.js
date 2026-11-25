// Tests de l'écran de progression (ProgressView.vue) : il reçoit les messages du serveur Python par socket.io
// (FragHub.py, emit_to_frontend) et les affiche. Le socket est remplacé par un faux socket qui rejoue ces messages.
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { useState } from '#imports'
import { flushPromises } from '@vue/test-utils'
import ActiveProgress from '~/components/ActiveProgress.vue'
import { resetParameters } from './outils.js'

// Faux socket : garde les fonctions de chaque message (socket.on) pour que le test les appelle comme le serveur
const fake = vi.hoisted(() => ({ handlers: {}, disconnected: false, url: null }))
vi.mock('socket.io-client', () => ({
  io: (url) => {
    fake.url = url
    return {
      on: (event, handler) => { fake.handlers[event] = handler },
      disconnect: () => { fake.disconnected = true },
    }
  },
}))
const { default: ProgressView } = await import('~/components/ProgressView.vue')

/** Envoie un message du serveur, puis attend la mise à jour de l'affichage */
async function emit(wrapper, event, data) {
  fake.handlers[event](data)
  await flushPromises()
  await wrapper.vm.$nextTick()
}

// Messages tels que FragHub.py les envoie : total_items et progress portent l'état de la tâche
let task = 0
function startTask(total, prefix = 'Parsing MSP spectrums:', itemType = 'spectra') {
  task += 1
  return { task, total, prefix, item_type: itemType, started: Date.now() }
}
const progress = (meta, value) => ({ value, ...meta })

let fetchMock
beforeEach(() => {
  resetParameters()
  useState('isExecuting', () => true).value = true
  fake.handlers = {}
  fake.disconnected = false
  task = 0
  fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({}) }))
  vi.stubGlobal('fetch', fetchMock)
})
afterEach(() => { vi.unstubAllGlobals() })

describe('Écran de progression', () => {
  it('se connecte au serveur local de FragHub', async () => {
    await mountSuspended(ProgressView)
    expect(fake.url).toBe('http://127.0.0.1:8000')
    for (const e of ['progress', 'total_items', 'prefix', 'item_type', 'step', 'deletion', 'completion']) {
      expect(fake.handlers[e], e).toBeTypeOf('function')
    }
  })

  it('étapes, suppressions et avancement de la tâche en cours', async () => {
    const w = await mountSuspended(ProgressView)
    await emit(w, 'step', '-- PARSING MSP TO DICT --')
    const meta = startTask(200)
    await emit(w, 'total_items', meta)
    await emit(w, 'prefix', meta.prefix)
    await emit(w, 'item_type', meta.item_type)
    await emit(w, 'progress', progress(meta, 50))
    const text = w.text()
    expect(text).toContain('-- PARSING MSP TO DICT --')
    expect(text).toContain('Parsing MSP spectrums')          // « : » final retiré
    expect(text).not.toContain('spectrums:')
    expect(text).toContain('50 of 200 spectra')
    expect(text).toContain('25.00%')
    await emit(w, 'deletion', 'previously cleaned: 3')
    expect(w.text()).toContain('previously cleaned: 3')
    expect(w.text()).toContain('STOP PROCESS')
  })

  it('tâche terminée : figée en vert, la suivante repart de zéro', async () => {
    const w = await mountSuspended(ProgressView)
    const first = startTask(10, 'Generating SPLASH IDs for [MSP]:')
    await emit(w, 'total_items', first)
    await emit(w, 'progress', progress(first, 10))
    expect(w.findAll('.finished-progress-zone').length).toBe(1)
    // 100 % envoyé deux fois (le serveur force toujours l'envoi du dernier message) : figée une seule fois
    await emit(w, 'progress', progress(first, 10))
    expect(w.findAll('.finished-progress-zone').length).toBe(1)
    expect(w.find('.finished-progress-zone').text()).toContain('Generating SPLASH IDs for [MSP]')
    expect(w.find('.finished-progress-zone').text()).toContain('10 of 10 spectra')
    const second = startTask(400, 'cleaning spectrums:')
    await emit(w, 'total_items', second)
    await emit(w, 'prefix', second.prefix)
    expect(w.find('.active-progress-zone').text()).toContain('0 of 400 spectra')
    expect(w.find('.active-progress-zone').text()).toContain('0.00%')
  })

  it('tâche suivante commencée avant la fin de la précédente : la précédente est figée, une seule fois', async () => {
    const w = await mountSuspended(ProgressView)
    const first = startTask(100)
    await emit(w, 'total_items', first)
    await emit(w, 'progress', progress(first, 40))   // la dernière progression a été filtrée par le serveur
    await emit(w, 'total_items', startTask(5))
    expect(w.findAll('.finished-progress-zone').length).toBe(1)
    expect(w.find('.finished-progress-zone').text()).toContain('100 of 100')
  })

  it('message de début de tâche perdu (reconnexion) : la progression suffit pour changer de tâche', async () => {
    const w = await mountSuspended(ProgressView)
    const first = startTask(10, 'A:')
    await emit(w, 'total_items', first)
    const second = startTask(300, 'Ontologies:', 'rows')      // son « total_items » n'arrive jamais
    await emit(w, 'progress', progress(second, 30))
    expect(w.findAll('.finished-progress-zone').length).toBe(1)
    const active = w.find('.active-progress-zone').text()
    expect(active).toContain('Ontologies')
    expect(active).toContain('30 of 300 rows')
    // « total_items » de cette tâche arrivé en retard : ignoré (la barre ne revient pas à zéro)
    await emit(w, 'total_items', second)
    expect(w.find('.active-progress-zone').text()).toContain('30 of 300 rows')
    // Progression en retard d'une tâche déjà dépassée : ignorée
    await emit(w, 'progress', progress(first, 2))
    expect(w.find('.active-progress-zone').text()).toContain('30 of 300 rows')
  })

  it('lecture d\'un fichier : avancement en MB ou GB, vitesse en MB/s', async () => {
    const w = await mountSuspended(ProgressView)
    const meta = startTask(2_048_000_000, 'loading [ALL_GNPS.json]:', 'bytes')
    await emit(w, 'total_items', meta)
    await emit(w, 'progress', progress(meta, 350_200_000))
    const active = w.find('.active-progress-zone').text()
    expect(active).toContain('0.35 of 2.05 GB')
    expect(active).toMatch(/[\d.,]+ (MB|GB)\/s/)
    expect(active).not.toContain('bytes')
  })

  it('écriture des sorties : spectres pour le pourcentage, octets écrits et MB/s à côté', async () => {
    const w = await mountSuspended(ProgressView)
    const meta = startTask(5000, 'Writing CSV files:', 'spectra')
    await emit(w, 'total_items', meta)
    await emit(w, 'progress', { ...progress(meta, 1200), bytes: 1_420_000_000 })
    const active = w.find('.active-progress-zone').text()
    expect(active).toContain('1200 of 5000 spectra | 1.42 GB written')
    expect(active).toMatch(/[\d.,]+ (MB|GB)\/s/)
    expect(active).not.toContain('spectra/s')
    // Tâche suivante sans octets : retour à l'affichage en éléments
    const next = startTask(10, 'Updating ontologies:', 'spectra')
    await emit(w, 'total_items', next)
    await emit(w, 'progress', progress(next, 4))
    expect(w.find('.active-progress-zone').text()).toContain('spectra/s')
    expect(w.find('.active-progress-zone').text()).not.toContain('written')
  })

  it('progression au-delà du total : barre bloquée à 100 %', async () => {
    const w = await mountSuspended(ProgressView)
    const meta = startTask(4)
    await emit(w, 'total_items', meta)
    await emit(w, 'progress', progress(meta, 9))
    expect(w.text()).not.toMatch(/2\d\d\.\d\d%/)
  })

  it('fin du run avec rapport : message, onglet du rapport HTML, bouton de fermeture', async () => {
    const w = await mountSuspended(ProgressView)
    const meta = startTask(3)
    await emit(w, 'total_items', meta)
    await emit(w, 'completion', { message: '--- TOTAL TIME: 00:01:02 ---', report_path: 'C:\\sortie\\report_13_11_2026.html' })
    expect(w.text()).toContain('--- TOTAL TIME: 00:01:02 ---')
    expect(w.text()).toContain('HTML RESULT')
    expect(w.text()).toContain('CLOSE REPORT')
    expect(w.text()).not.toContain('STOP PROCESS')
    // Onglet du rapport : servi par le serveur Python, chemin encodé dans l'adresse
    const tab = w.findAll('.v-tab').find((t) => t.text().includes('HTML RESULT'))
    await tab.trigger('click')
    await flushPromises()
    expect(w.find('iframe').attributes('src'))
      .toBe('http://127.0.0.1:8000/report?path=' + encodeURIComponent('C:\\sortie\\report_13_11_2026.html'))
    // Fermeture : retour aux onglets de réglage
    await w.findAll('button').find((b) => b.text().includes('CLOSE REPORT')).trigger('click')
    expect(useState('isExecuting').value).toBe(false)
  })

  it('fin sans rapport (arrêt, rien à traiter) : pas d\'onglet du rapport', async () => {
    const w = await mountSuspended(ProgressView)
    await emit(w, 'completion', { message: 'Process stopped by user', report_path: '' })
    expect(w.text()).toContain('Process stopped by user')
    expect(w.text()).not.toContain('HTML RESULT')
    await emit(w, 'completion', 'Ancien format : un simple texte')
    expect(w.text()).toContain('Ancien format : un simple texte')
    expect(w.text()).not.toContain('HTML RESULT')
  })

  it('bouton STOP : demande l\'arrêt au serveur, une fois, et l\'affiche', async () => {
    const w = await mountSuspended(ProgressView)
    await w.findAll('button').find((b) => b.text().includes('STOP PROCESS')).trigger('click')
    await flushPromises()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:8000/stop-analysis')
    expect(w.text()).toContain('STOPPING...')
  })

  it('rapport remonté pendant le traitement : il ne redescend plus tout seul', async () => {
    const w = await mountSuspended(ProgressView)
    // happy-dom ne calcule pas de mise en page : zone de 200 px de haut pour 1 000 px de rapport
    const el = w.find('.v-card-text').element
    let top = 800
    Object.defineProperty(el, 'scrollHeight', { configurable: true, get: () => 1000 })
    Object.defineProperty(el, 'clientHeight', { configurable: true, get: () => 200 })
    Object.defineProperty(el, 'scrollTop', { configurable: true, get: () => top, set: (v) => { top = v } })
    el.scrollTo = vi.fn(({ top: t }) => { top = Math.min(t, 800) })
    const meta = startTask(100)
    await emit(w, 'total_items', meta)

    // En bas : le rapport suit les nouvelles lignes
    await emit(w, 'progress', progress(meta, 10))
    expect(el.scrollTo).toHaveBeenLastCalledWith({ top: 1000, behavior: 'auto' })
    expect(w.text()).not.toContain('Back to latest')

    // L'utilisateur remonte : plus aucun défilement automatique, le bouton apparaît
    top = 300
    await w.find('.v-card-text').trigger('scroll')
    el.scrollTo.mockClear()
    await emit(w, 'progress', progress(meta, 20))
    await emit(w, 'step', 'Une étape')
    expect(el.scrollTo).not.toHaveBeenCalled()
    expect(top).toBe(300)
    const back = w.findAll('button').find((b) => b.text().includes('Back to latest'))
    expect(back).toBeTruthy()

    // Le bouton ramène en bas et relance le suivi
    await back.trigger('click')
    await flushPromises()
    expect(top).toBe(800)
    expect(w.text()).not.toContain('Back to latest')
    el.scrollTo.mockClear()
    await emit(w, 'progress', progress(meta, 30))
    expect(el.scrollTo).toHaveBeenCalled()

    // Revenir en bas à la main relance aussi le suivi
    top = 100
    await w.find('.v-card-text').trigger('scroll')
    expect(w.text()).toContain('Back to latest')
    top = 780
    await w.find('.v-card-text').trigger('scroll')
    expect(w.text()).not.toContain('Back to latest')
  })

  it('écran fermé : socket déconnecté', async () => {
    const w = await mountSuspended(ProgressView)
    w.unmount()
    expect(fake.disconnected).toBe(true)
  })
})

describe('Barre de progression active', () => {
  it('texte de l\'étape sans le « : » final, compte, durée, reste et vitesse', async () => {
    const w = await mountSuspended(ActiveProgress, { props: {
      prefix: 'Removing duplicates:  ', progressPercent: 37.5, counts: '3 of 8 spectra',
      elapsed: '00:00:02.500', eta: '00:00:04', rate: '1.20 spectra/s' } })
    const text = w.text()
    expect(text).toContain('Removing duplicates')
    expect(text).not.toContain('duplicates:')
    expect(text).toContain('37.50%')
    expect(text).toContain('3 of 8 spectra')
    expect(text).toContain('Elapsed: 00:00:02.500')
    expect(text).toContain('ETA: 00:00:04')
    expect(text).toContain('1.20 spectra/s')
  })
})
