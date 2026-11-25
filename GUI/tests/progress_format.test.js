// Tests des textes des barres de progression (app/utils/progress_format.js)
import { describe, it, expect } from 'vitest'
import { formatSize, formatRate, progressTexts, sizeUnit } from '~/utils/progress_format.js'

describe('progress_format', () => {
  it('tailles en MB sous 1 GB, en GB au-delà (unités décimales)', () => {
    expect(sizeUnit(999_999_999)).toBe('MB')
    expect(sizeUnit(1_000_000_000)).toBe('GB')
    expect(formatSize(350_200_000)).toBe('350.2 MB')
    expect(formatSize(1_420_000_000)).toBe('1.42 GB')
    expect(formatSize(2_048_000_000, 'MB')).toBe('2,048.0 MB')
    expect(formatRate(120_400_000)).toBe('120.4 MB/s')
    expect(formatRate(1_350_000_000)).toBe('1.35 GB/s')
  })

  it('lecture : avancement et total dans la même unité, choisie d\'après le total', () => {
    expect(progressTexts({ current: 350_200_000, total: 800_000_000, itemType: 'bytes', elapsedSec: 2 }))
      .toEqual({ counts: '350.2 of 800.0 MB', rate: '175.1 MB/s' })
    expect(progressTexts({ current: 350_200_000, total: 2_048_000_000, itemType: 'bytes', elapsedSec: 2 }).counts)
      .toBe('0.35 of 2.05 GB')
  })

  it('écriture : spectres et octets écrits, vitesse en octets', () => {
    expect(progressTexts({ current: 1200, total: 5000, itemType: 'spectra', elapsedSec: 4, writtenBytes: 1_420_000_000 }))
      .toEqual({ counts: '1200 of 5000 spectra | 1.42 GB written', rate: '355.0 MB/s' })
  })

  it('autres étapes : éléments et éléments par seconde, comme avant', () => {
    expect(progressTexts({ current: 30, total: 300, itemType: 'rows', elapsedSec: 2 }))
      .toEqual({ counts: '30 of 300 rows', rate: '15.00 rows/s' })
    expect(progressTexts({ current: 0, total: 10, itemType: 'spectra', elapsedSec: 0 }).rate).toBe('0.00 spectra/s')
  })
})
