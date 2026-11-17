import { describe, expect, it } from 'vitest'
import { formatAxis, formatUnit, periodError, rangeToFetch, sampleForTable, share, withinPeriod, type Period } from './period'

const custom = (from: string, to: string): Period => ({ kind: 'custom', from, to })
const days = ['2026-01-30', '2026-01-31', '2026-02-01', '2026-02-15', '2026-03-01'].map((date) => ({ date }))

describe('période', () => {
  it('demande la période prédéfinie à l’API, et tout l’historique pour une période personnalisée', () => {
    expect(rangeToFetch({ kind: 'preset', range: '3m' })).toBe('3m')
    expect(rangeToFetch(custom('2026-01-01', '2026-02-01'))).toBe('all')
  })

  it('valide les bornes d’une période personnalisée', () => {
    expect(periodError({ kind: 'preset', range: 'all' })).toBeNull()
    expect(periodError(custom('2026-01-01', '2026-01-01'))).toBeNull()
    expect(periodError(custom('', '2026-01-01'))).toBe('Indiquez une date de début et une date de fin.')
    expect(periodError(custom('2026-02-01', '2026-01-01'))).toBe('La date de début doit précéder la date de fin.')
  })

  it('garde les points de la période, bornes incluses', () => {
    expect(withinPeriod(days, custom('2026-01-31', '2026-02-15')).map((d) => d.date)).toEqual([
      '2026-01-31',
      '2026-02-01',
      '2026-02-15',
    ])
    expect(withinPeriod(days, { kind: 'preset', range: '1y' })).toHaveLength(5)
    expect(withinPeriod(days, custom('2026-03-01', '2026-01-01'))).toEqual([])
  })

  it('résume un long historique à la fin de chaque mois pour le tableau', () => {
    expect(sampleForTable(days, 10)).toHaveLength(5)
    expect(sampleForTable(days, 2).map((d) => d.date)).toEqual(['2026-01-31', '2026-02-15', '2026-03-01'])
  })

  it('calcule une part, sans diviser par zéro', () => {
    expect(share(25, 200)).toBe(12.5)
    expect(share(-50, -200)).toBe(-25)
    expect(share(10, 0)).toBeNull()
  })

  it('formate selon l’unité choisie', () => {
    const plain = (text: string) => text.replace(/[\u00a0\u202f]/g, ' ')
    expect(plain(formatUnit(1234.4, 'eur'))).toBe('1 234 €')
    expect(plain(formatUnit(250, 'eur', true))).toBe('+250 €')
    expect(plain(formatUnit(12.34, 'pct', true))).toBe('+12,3 %')
    expect(plain(formatUnit(null, 'pct'))).toBe('–')
    expect(plain(formatAxis(35, 'pct'))).toBe('35 %')
  })
})
