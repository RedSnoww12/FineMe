import { describe, expect, it } from 'vitest'
import type { HistoryAccount, HistoryPoint } from '@/shared/api/types'
import { countEstimatedDays, fillMissingDays, toHistoryChartRows } from './history'

function point(date: string, gross_assets: number, liabilities = 0): HistoryPoint {
  return { date, gross_assets, liabilities, net_worth: gross_assets - liabilities, accounts: null }
}

function account(account_id: string, value: number, gain: number | null = null): HistoryAccount {
  return { account_id, name: `Compte ${account_id}`, kind: 'investment', value, gain }
}

describe('fillMissingDays', () => {
  it('renvoie une série vide sans instantané', () => {
    expect(fillMissingDays([])).toEqual([])
  })

  it('garde tels quels des jours consécutifs, sans estimation', () => {
    const series = fillMissingDays([point('2026-03-01', 1000), point('2026-03-02', 1100)])
    expect(series).toEqual([
      { ...point('2026-03-01', 1000), estimated: false },
      { ...point('2026-03-02', 1100), estimated: false },
    ])
    expect(countEstimatedDays(series)).toBe(0)
  })

  it('interpole linéairement les jours manquants entre deux instantanés', () => {
    const series = fillMissingDays([point('2026-03-01', 1000, 400), point('2026-03-05', 1400, 200)])
    expect(series.map((p) => p.date)).toEqual(['2026-03-01', '2026-03-02', '2026-03-03', '2026-03-04', '2026-03-05'])
    expect(series.map((p) => p.estimated)).toEqual([false, true, true, true, false])
    expect(series[2]).toEqual({
      date: '2026-03-03',
      gross_assets: 1200,
      liabilities: 300,
      net_worth: 900,
      accounts: null,
      estimated: true,
    })
    // Le patrimoine net estimé reste égal aux actifs moins les dettes estimés.
    for (const p of series) expect(p.net_worth).toBeCloseTo(p.gross_assets - p.liabilities, 9)
    expect(countEstimatedDays(series)).toBe(3)
  })

  it('interpole le détail par compte, un compte ouvert ou clôturé valant 0 de l’autre côté', () => {
    const series = fillMissingDays([
      { ...point('2026-03-01', 1000), accounts: [account('pea', 600, 100), account('livret', 400)] },
      { ...point('2026-03-03', 1400), accounts: [account('pea', 1000, 300), account('cto', 400, 50)] },
    ])
    expect(series[1].estimated).toBe(true)
    expect(series[1].accounts).toEqual([
      account('pea', 800, 200),
      account('livret', 200),
      account('cto', 200, 25),
    ])
  })

  it('n’invente pas de détail pour un jour encadré par un instantané sans détail', () => {
    const series = fillMissingDays([
      point('2026-03-01', 1000),
      { ...point('2026-03-03', 1400), accounts: [account('pea', 1400)] },
    ])
    expect(series[1].accounts).toBeNull()
    expect(series[2].accounts).toEqual([account('pea', 1400)])
  })

  it('franchit les fins de mois, d’année et les années bissextiles', () => {
    expect(fillMissingDays([point('2028-02-27', 0), point('2028-03-01', 0)]).map((p) => p.date)).toEqual([
      '2028-02-27',
      '2028-02-28',
      '2028-02-29',
      '2028-03-01',
    ])
    expect(fillMissingDays([point('2026-12-31', 0), point('2027-01-02', 0)]).map((p) => p.date)).toEqual([
      '2026-12-31',
      '2027-01-01',
      '2027-01-02',
    ])
  })

  it('n’est pas décalé par le passage à l’heure d’été', () => {
    const series = fillMissingDays([point('2026-03-28', 0), point('2026-03-31', 0)])
    expect(series.map((p) => p.date)).toEqual(['2026-03-28', '2026-03-29', '2026-03-30', '2026-03-31'])
  })

  it('n’extrapole ni avant le premier ni après le dernier instantané', () => {
    const series = fillMissingDays([point('2026-03-10', 500)])
    expect(series).toEqual([{ ...point('2026-03-10', 500), estimated: false }])
  })

  it('trie les instantanés, ignore les doublons et les dates illisibles', () => {
    const series = fillMissingDays([
      point('2026-03-03', 300),
      point('pas-une-date', 999),
      point('2026-03-01', 100),
      point('2026-03-01', 777),
    ])
    expect(series.map((p) => [p.date, p.net_worth, p.estimated])).toEqual([
      ['2026-03-01', 100, false],
      ['2026-03-02', 200, true],
      ['2026-03-03', 300, false],
    ])
  })
})

describe('toHistoryChartRows', () => {
  it('relie le tracé estimé aux instantanés qui encadrent le trou', () => {
    const series = fillMissingDays([
      point('2026-03-01', 100),
      point('2026-03-02', 200),
      point('2026-03-04', 400),
      point('2026-03-05', 500),
    ])
    expect(toHistoryChartRows(series).map((row) => [row.date, row.measured, row.estimate])).toEqual([
      ['2026-03-01', 100, null],
      ['2026-03-02', 200, 200],
      ['2026-03-03', null, 300],
      ['2026-03-04', 400, 400],
      ['2026-03-05', 500, null],
    ])
  })

  it('ne trace aucune estimation sur un historique complet', () => {
    const rows = toHistoryChartRows(fillMissingDays([point('2026-03-01', 100), point('2026-03-02', 200)]))
    expect(rows.every((row) => row.estimate === null)).toBe(true)
  })
})
