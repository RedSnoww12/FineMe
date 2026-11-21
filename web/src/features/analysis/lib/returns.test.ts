import { describe, expect, it } from 'vitest'
import type { Performance } from '@/shared/api/types'
import {
  benchmarkInput,
  curveRows,
  EMPTY_BENCHMARK_FORM,
  formatMeasure,
  isPartialYear,
  MEASURES,
  performanceBounds,
  undefinedReason,
  yearsNewestFirst,
} from './returns'

const performance: Performance = {
  from: '2024-06-01',
  to: '2025-06-30',
  start_value: 1000,
  end_value: 1300,
  net_flows: 100,
  gain: 200,
  twr: 0.21,
  twr_annualized: 0.19,
  mwr: 0.2,
  mwr_annualized: 0.18,
  years: [
    { year: 2024, from: '2024-06-01', to: '2024-12-31', twr: 0.1 },
    { year: 2025, from: '2024-12-31', to: '2025-06-30', twr: 0.1 },
  ],
  points: [
    { date: '2024-06-01', value: 100, benchmark: null },
    { date: '2025-06-30', value: 121, benchmark: 115 },
  ],
}

describe('bornes de la période', () => {
  it('remonte du nombre de mois du préréglage, en restant dans le mois visé', () => {
    expect(performanceBounds({ kind: 'preset', range: '1m' }, '2026-03-31')).toEqual({ from: '2026-02-28' })
    expect(performanceBounds({ kind: 'preset', range: '1y' }, '2026-03-15')).toEqual({ from: '2025-03-15' })
    expect(performanceBounds({ kind: 'preset', range: '3m' }, '2026-01-10')).toEqual({ from: '2025-10-10' })
    expect(performanceBounds({ kind: 'preset', range: 'all' }, '2026-03-15')).toEqual({})
  })

  it('reprend une période personnalisée valide et refuse les autres', () => {
    expect(performanceBounds({ kind: 'custom', from: '2025-01-01', to: '2025-12-31' }, '2026-01-01')).toEqual({
      from: '2025-01-01',
      to: '2025-12-31',
    })
    expect(performanceBounds({ kind: 'custom', from: '2025-12-31', to: '2025-01-01' }, '2026-01-01')).toBeNull()
  })
})

describe('présentation des mesures', () => {
  it('explique chaque mesure en une phrase', () => {
    for (const measure of MEASURES) {
      expect(measure.explanation.split(/[.!?](\s|$)/).filter((part) => part.trim()).length).toBe(1)
    }
  })

  it('affiche une mesure indéfinie explicitement, jamais zéro', () => {
    expect(formatMeasure(null)).toBe('Indéfini')
    expect(formatMeasure(0.123)).toMatch(/^\+12,3\s%$/)
    expect(formatMeasure(-1)).toMatch(/^-100,0\s%$/)
    expect(undefinedReason('twr')).toContain('deux relevés')
    expect(undefinedReason('mwr_annualized')).toContain('un an')
  })

  it('signale les années partielles et range la plus récente en premier', () => {
    const [latest, previous] = yearsNewestFirst(performance)
    expect(latest.year).toBe(2025)
    expect(isPartialYear(latest)).toBe(true)
    expect(isPartialYear(previous)).toBe(true)
    expect(isPartialYear({ year: 2025, from: '2024-12-31', to: '2025-12-31', twr: 0 })).toBe(false)
  })

  it('trace le portefeuille et l’indice, absent hors de sa fenêtre', () => {
    expect(curveRows(performance)).toEqual([
      { date: '2024-06-01', portfolio: 100, benchmark: null },
      { date: '2025-06-30', portfolio: 121, benchmark: 115 },
    ])
  })
})

describe('formulaire de l’indice', () => {
  it('exige un nom, un identifiant et une devise valide', () => {
    expect(benchmarkInput(EMPTY_BENCHMARK_FORM)).toBe('Donnez un nom à l’indice.')
    expect(benchmarkInput({ ...EMPTY_BENCHMARK_FORM, name: 'Monde' })).toContain('ticker ou un ISIN')
    expect(benchmarkInput({ ...EMPTY_BENCHMARK_FORM, name: 'Monde', isin: 'x', currency: 'euro' })).toContain('Devise')
  })

  it('nettoie les champs et omet ceux laissés vides', () => {
    expect(benchmarkInput({ ...EMPTY_BENCHMARK_FORM, name: ' Monde ', isin: ' ie00demo0001 ', currency: 'usd' })).toEqual({
      name: 'Monde',
      isin: 'IE00DEMO0001',
      kind: 'security',
      currency: 'USD',
    })
  })
})
