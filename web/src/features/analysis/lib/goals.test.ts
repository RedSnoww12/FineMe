import { describe, expect, it } from 'vitest'
import { futureValue } from '@/shared/lib/compound'
import { fillMissingDays } from '@/shared/lib/history'
import { buildGoal, buildHistoryAccount as line, buildHistoryPoint as point } from '@/test/fixtures'
import { gapToPlan, goalChartRows, goalPlannedSeries, goalRealSeries } from './goals'

const goal = buildGoal({
  account_ids: ['livret', 'pea'],
  target_amount: 20_000,
  target_date: '2026-06-15',
  monthly_contribution: 500,
  expected_return_pct: 0,
})

describe('trajectoire réelle', () => {
  it('additionne la valeur des comptes liés, jour par jour, sans les dettes ni les autres comptes', () => {
    const series = fillMissingDays([
      point('2026-01-01', [
        line({ account_id: 'livret', kind: 'savings', value: 5000 }),
        line({ account_id: 'pea', kind: 'investment', value: 3000 }),
        line({ account_id: 'courant', value: 9000 }),
      ]),
      point('2026-01-03', [line({ account_id: 'livret', kind: 'savings', value: 6000 })]),
    ])
    expect(goalRealSeries(goal, series)).toEqual([
      { date: '2026-01-01', amount: 8000 },
      { date: '2026-01-02', amount: 7000 },
      { date: '2026-01-03', amount: 6000 },
    ])
  })

  it('saute les jours sans détail par compte', () => {
    expect(goalRealSeries(goal, fillMissingDays([point('2026-01-01', null)]))).toEqual([])
  })
})

describe('trajectoire prévue', () => {
  it('ajoute chaque mois le versement prévu jusqu’à l’échéance incluse', () => {
    const planned = goalPlannedSeries(goal, { date: '2026-01-10', amount: 8000 })
    expect(planned.map((p) => [p.date, p.amount])).toEqual([
      ['2026-01-10', 8000],
      ['2026-02-10', 8500],
      ['2026-03-10', 9000],
      ['2026-04-10', 9500],
      ['2026-05-10', 10_000],
      ['2026-06-10', 10_500],
      ['2026-06-15', 10_500],
    ])
  })

  it('capitalise au rendement attendu et gère les fins de mois', () => {
    const growing = buildGoal({ ...goal, expected_return_pct: 6, target_date: '2026-03-31' })
    const planned = goalPlannedSeries(growing, { date: '2026-01-31', amount: 10_000 })
    expect(planned.map((p) => p.date)).toEqual(['2026-01-31', '2026-02-28', '2026-03-31'])
    expect(planned[2].amount).toBeCloseTo(futureValue(10_000, 500, 6, 2), 9)
  })

  it('ne prévoit rien pour une épargne de précaution, un projet sans échéance ou une échéance passée', () => {
    const start = { date: '2026-01-01', amount: 1000 }
    expect(goalPlannedSeries(buildGoal({ kind: 'safety_net', target_date: null }), start)).toEqual([])
    expect(goalPlannedSeries(buildGoal({ target_date: null }), start)).toEqual([])
    expect(goalPlannedSeries(buildGoal({ target_date: '2025-12-31' }), start)).toEqual([])
  })
})

describe('graphique et écart au plan', () => {
  const real = [
    { date: '2026-01-10', amount: 8000 },
    { date: '2026-01-25', amount: 8100 },
  ]
  const planned = goalPlannedSeries(goal, real[0])

  it('fusionne les deux trajectoires par date, en euros ou en pourcentage de la cible', () => {
    const rows = goalChartRows(real, planned, 20_000, 'eur')
    expect(rows.slice(0, 3)).toEqual([
      { date: '2026-01-10', real: 8000, planned: 8000 },
      { date: '2026-01-25', real: 8100, planned: null },
      { date: '2026-02-10', real: null, planned: 8500 },
    ])
    expect(goalChartRows(real, planned, 20_000, 'pct')[0]).toEqual({ date: '2026-01-10', real: 40, planned: 40 })
  })

  it('mesure l’écart du dernier montant réel à la trajectoire prévue au même jour', () => {
    // Le 25 janvier, le plan interpole entre 8 000 € (10/01) et 8 500 € (10/02) : 8 000 + 500 × 15/31.
    expect(gapToPlan(real, planned)).toBeCloseTo(8100 - (8000 + (500 * 15) / 31), 9)
    expect(gapToPlan(real.slice(0, 1), planned)).toBe(0)
    expect(gapToPlan(real, [])).toBeNull()
    expect(gapToPlan([], planned)).toBeNull()
  })
})
