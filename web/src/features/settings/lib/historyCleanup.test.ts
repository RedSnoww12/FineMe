import { describe, expect, it } from 'vitest'
import type { HistoryReviewPoint } from '@/shared/api/types'
import {
  countInRange,
  isCorrectable,
  isResetConfirmed,
  newestFirst,
  parseCorrection,
  rangeErrors,
  RESET_WORD,
  suspiciousPoints,
} from './historyCleanup'

function point(date: string, overrides: Partial<HistoryReviewPoint> = {}): HistoryReviewPoint {
  return {
    date,
    gross_assets: 1000,
    liabilities: 0,
    net_worth: 1000,
    detailed: true,
    corrected: false,
    change: null,
    suspicious: false,
    ...overrides,
  }
}

const points = [point('2026-03-01'), point('2026-03-02', { suspicious: true }), point('2026-03-04')]

describe('confirmation de la remise à zéro', () => {
  it('exige le mot exact, espaces autour ignorés', () => {
    expect(isResetConfirmed(RESET_WORD)).toBe(true)
    expect(isResetConfirmed('  EFFACER ')).toBe(true)
    expect(isResetConfirmed('effacer')).toBe(false)
    expect(isResetConfirmed('EFFACE')).toBe(false)
    expect(isResetConfirmed('')).toBe(false)
  })
})

describe('sélection des points', () => {
  it('trie du plus récent au plus ancien sans modifier la liste', () => {
    expect(newestFirst(points).map((p) => p.date)).toEqual(['2026-03-04', '2026-03-02', '2026-03-01'])
    expect(points[0].date).toBe('2026-03-01')
  })

  it('isole les points suspects', () => {
    expect(suspiciousPoints(points).map((p) => p.date)).toEqual(['2026-03-02'])
  })

  it('compte les points d’une période, bornes incluses', () => {
    expect(countInRange(points, '2026-03-02', '2026-03-04')).toBe(2)
    expect(countInRange(points, '2026-03-03', '2026-03-03')).toBe(0)
    expect(countInRange(points, '2026-03-01', '2026-03-01')).toBe(1)
  })

  it('ne propose pas de corriger le point du jour', () => {
    expect(isCorrectable(point('2026-03-03'), '2026-03-04')).toBe(true)
    expect(isCorrectable(point('2026-03-04'), '2026-03-04')).toBe(false)
  })
})

describe('rangeErrors', () => {
  it('accepte une période ordonnée ou un seul jour', () => {
    expect(rangeErrors('2026-03-01', '2026-03-04')).toEqual({})
    expect(rangeErrors('2026-03-01', '2026-03-01')).toEqual({})
  })

  it('signale les dates manquantes et une fin avant le début', () => {
    expect(rangeErrors('', '')).toEqual({ from: 'Indiquez la date de début.', to: 'Indiquez la date de fin.' })
    expect(rangeErrors('2026-03-04', '2026-03-01')).toEqual({ to: 'La date de fin doit suivre la date de début.' })
  })
})

describe('parseCorrection', () => {
  it('lit les montants au format français', () => {
    expect(parseCorrection('101 500,50', '1 000')).toEqual({
      input: { gross_assets: 101_500.5, liabilities: 1000 },
      errors: null,
    })
    expect(parseCorrection('0', '0').input).toEqual({ gross_assets: 0, liabilities: 0 })
  })

  it('refuse un montant absent ou négatif', () => {
    expect(parseCorrection('', '-5')).toEqual({
      input: null,
      errors: { gross_assets: 'Indiquez les actifs bruts.', liabilities: 'Les dettes ne peuvent pas être négatives.' },
    })
    expect(parseCorrection('abc', '0').errors).toEqual({ gross_assets: 'Indiquez les actifs bruts.' })
  })
})
