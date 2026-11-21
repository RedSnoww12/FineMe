import { describe, expect, it } from 'vitest'
import type { ScheduleRow, Valuation } from '@/shared/api/types'
import { newestFirst, remainingOn, valuationOn, valuationSeries, valueFromPricePerM2 } from './valuation'

function valuation(valued_on: string, value: number, recorded_at = `${valued_on}T10:00:00Z`): Valuation {
  return { id: `${valued_on}-${value}`, account_id: 'a', valued_on, value, source: 'manual', price_per_m2: null, note: null, recorded_at }
}

function row(date: string, remaining: number): ScheduleRow {
  return { month: 0, date, payment: 0, interest: 0, principal: 0, insurance: 0, remaining }
}

const history = [valuation('2024-01-10', 200_000), valuation('2026-03-01', 230_000), valuation('2025-01-01', 210_000)]

describe('valuationOn', () => {
  it('retient la plus récente au plus tard à la date', () => {
    expect(valuationOn(history, '2026-06-01')?.value).toBe(230_000)
    expect(valuationOn(history, '2025-06-01')?.value).toBe(210_000)
    expect(valuationOn(history, '2023-01-01')).toBeNull()
    expect(valuationOn([], '2026-01-01')).toBeNull()
  })

  it('départage deux estimations du même jour par la saisie', () => {
    const sameDay = [valuation('2026-01-01', 1, '2026-01-01T09:00:00Z'), valuation('2026-01-01', 2, '2026-01-01T11:00:00Z')]
    expect(valuationOn(sameDay, '2026-01-01')?.value).toBe(2)
    expect(newestFirst(history).map((v) => v.value)).toEqual([230_000, 210_000, 200_000])
  })
})

describe('valueFromPricePerM2', () => {
  it('multiplie la surface par le prix', () => {
    expect(valueFromPricePerM2(42.5, 4000)).toBe(170_000)
  })
  it('refuse une surface ou un prix absent ou non positif', () => {
    expect(valueFromPricePerM2(null, 4000)).toBeNull()
    expect(valueFromPricePerM2(40, 0)).toBeNull()
    expect(valueFromPricePerM2(-1, 4000)).toBeNull()
  })
})

describe('remainingOn', () => {
  const loan = { loan: { principal: 1000 }, rows: [row('2025-02-01', 900), row('2025-03-01', 800), row('2025-04-01', 0)] }
  it('vaut le capital emprunté avant la première échéance, puis suit le tableau', () => {
    expect(remainingOn(loan, '2025-01-15')).toBe(1000)
    expect(remainingOn(loan, '2025-02-01')).toBe(900)
    expect(remainingOn(loan, '2025-03-20')).toBe(800)
    expect(remainingOn(loan, '2030-01-01')).toBe(0)
  })
})

describe('valuationSeries', () => {
  it('une ligne par estimation et aujourd’hui, avec la valeur nette des prêts liés', () => {
    const loan = { loan: { principal: 100_000 }, rows: [row('2025-06-01', 90_000), row('2026-06-01', 80_000)] }
    const points = valuationSeries(history, [loan], '2026-07-01')
    expect(points).toEqual([
      { date: '2024-01-10', value: 200_000, debt: 100_000, net: 100_000 },
      { date: '2025-01-01', value: 210_000, debt: 100_000, net: 110_000 },
      { date: '2026-03-01', value: 230_000, debt: 90_000, net: 140_000 },
      { date: '2026-07-01', value: 230_000, debt: 80_000, net: 150_000 },
    ])
  })

  it('sans prêt, la valeur nette est la valeur ; sans estimation, aucune ligne', () => {
    expect(valuationSeries([valuation('2026-07-01', 5)], [], '2026-07-01')).toEqual([{ date: '2026-07-01', value: 5, debt: 0, net: 5 }])
    expect(valuationSeries([], [], '2026-07-01')).toEqual([])
  })
})
