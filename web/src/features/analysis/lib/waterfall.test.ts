import { describe, expect, it } from 'vitest'
import { buildHistoryAccount as line, buildHistoryPoint as point } from '@/test/fixtures'
import { periodVariation, waterfallSteps } from './waterfall'

// Début : 10 000 € de PEA (dont 1 000 € de plus-value), 2 000 € de courant, 5 000 € de prêt → 7 000 € net.
const start = point('2026-01-01', [
  line({ account_id: 'pea', kind: 'investment', value: 10_000, gain: 1000 }),
  line({ account_id: 'courant', kind: 'checking', value: 2000 }),
  line({ account_id: 'pret', kind: 'loan', value: 5000 }),
])
// Fin : le PEA a gagné 1 500 € de plus-value et reçu 500 € de versement ; 800 € de capital remboursés
// depuis le courant, qui a aussi reçu 300 € de salaire net → 12 000 + 1 500 − 4 200 = 9 300 € net.
const end = point('2026-02-01', [
  line({ account_id: 'pea', kind: 'investment', value: 12_000, gain: 2500 }),
  line({ account_id: 'courant', kind: 'checking', value: 1500 }),
  line({ account_id: 'pret', kind: 'loan', value: 4200 }),
])

describe('variation sur la période', () => {
  it('décompose l’écart en apports, performance et remboursement de dettes', () => {
    const v = periodVariation(start, end)
    expect(v.start).toBe(7000)
    expect(v.end).toBe(9300)
    expect(v.performance).toBe(1500)
    expect(v.debt_repayment).toBe(800)
    expect(v.contributions).toBe(0)
    expect(v.contributions + (v.performance ?? 0) + v.debt_repayment).toBe(v.end - v.start)
  })

  it('compte une nouvelle dette comme un remboursement négatif', () => {
    const borrowed = point('2026-02-01', [...(start.accounts ?? []), line({ account_id: 'conso', kind: 'loan', value: 3000 })])
    expect(periodVariation(start, borrowed).debt_repayment).toBe(-3000)
  })

  it('ne connaît pas la performance sans détail par compte, et la laisse dans les apports', () => {
    const old = point('2026-01-01', null, { gross_assets: 12_000, liabilities: 5000, net_worth: 7000 })
    const v = periodVariation(old, end)
    expect(v.performance).toBeNull()
    expect(v.contributions).toBe(2300 - 800)
  })
})

describe('barres de la cascade', () => {
  it('enchaîne les barres flottantes du début à la fin', () => {
    const steps = waterfallSteps(periodVariation(start, end), 'eur')
    expect(steps?.map((s) => [s.key, s.amount, s.range, s.kind])).toEqual([
      ['start', 7000, [0, 7000], 'total'],
      ['contributions', 0, [7000, 7000], 'up'],
      ['performance', 1500, [7000, 8500], 'up'],
      ['debt_repayment', 800, [8500, 9300], 'up'],
      ['end', 9300, [0, 9300], 'total'],
    ])
  })

  it('fait descendre une baisse et accepte un patrimoine net négatif', () => {
    const steps = waterfallSteps({ start: 1000, end: -500, contributions: -1500, performance: null, debt_repayment: 0 }, 'eur')
    expect(steps?.map((s) => s.key)).toEqual(['start', 'contributions', 'debt_repayment', 'end'])
    expect(steps?.[1]).toMatchObject({ amount: -1500, range: [-500, 1000], kind: 'down' })
    expect(steps?.[3].range).toEqual([-500, 0])
  })

  it('exprime les montants en pourcentage du patrimoine net de début', () => {
    const steps = waterfallSteps(periodVariation(start, end), 'pct')
    expect(steps?.[2].amount).toBeCloseTo((1500 / 7000) * 100, 9)
    expect(steps?.[4].amount).toBeCloseTo((9300 / 7000) * 100, 9)
    expect(waterfallSteps({ start: 0, end: 10, contributions: 10, performance: 0, debt_repayment: 0 }, 'pct')).toBeNull()
  })
})
