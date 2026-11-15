import { describe, expect, it } from 'vitest'
import { futureValue, monthlyRate } from '@/shared/lib/compound'
import { simulateFees, type FeeScenario } from './fees'

const noFees: FeeScenario = {
  initial: 10_000,
  monthly: 200,
  years: 20,
  grossReturnPct: 6,
  entryFeePct: 0,
  managementFeePct: 0,
  terPct: 0,
  brokerage: { mode: 'fixed', value: 0 },
  performanceFeePct: 0,
}

describe('simulateFees', () => {
  it('sans frais, retrouve la capitalisation simple', () => {
    const result = simulateFees(noFees)
    const expected = futureValue(10_000, 200, 6, 240)
    expect(result.finalValue).toBeCloseTo(expected, 4)
    expect(result.baselineValue).toBeCloseTo(expected, 4)
    expect(result.totalFees).toBe(0)
    expect(result.lostToFees).toBeCloseTo(0, 6)
    expect(result.contributed).toBe(10_000 + 200 * 240)
    expect(result.rows).toHaveLength(21)
  })

  it('prélève les frais d’entrée sur chaque versement', () => {
    const result = simulateFees({ ...noFees, grossReturnPct: 0, entryFeePct: 3, years: 1 })
    expect(result.totalFees).toBeCloseTo((10_000 + 200 * 12) * 0.03, 8)
    expect(result.finalValue).toBeCloseTo((10_000 + 200 * 12) * 0.97, 8)
  })

  it('compte un ordre de bourse par versement', () => {
    const fixed = simulateFees({ ...noFees, grossReturnPct: 0, years: 1, brokerage: { mode: 'fixed', value: 1 } })
    expect(fixed.totalFees).toBeCloseTo(13, 8)
    const percent = simulateFees({ ...noFees, grossReturnPct: 0, years: 1, brokerage: { mode: 'percent', value: 0.5 } })
    expect(percent.totalFees).toBeCloseTo((10_000 + 2400) * 0.005, 8)
  })

  it('les frais annuels réduisent le rendement d’autant', () => {
    const result = simulateFees({ ...noFees, monthly: 0, managementFeePct: 0.6, terPct: 0.4, years: 10 })
    // (1 + r_mensuel) × (1 − 1 %/12) chaque mois
    const factor = Math.pow((1 + monthlyRate(6)) * (1 - 0.01 / 12), 120)
    expect(result.finalValue).toBeCloseTo(10_000 * factor, 4)
  })

  it('la valeur perdue dépasse les frais payés (rendement manqué)', () => {
    const result = simulateFees({ ...noFees, entryFeePct: 2, managementFeePct: 0.8, terPct: 1.8 })
    expect(result.totalFees).toBeGreaterThan(0)
    expect(result.lostToFees).toBeGreaterThan(result.totalFees)
    expect(result.finalValue + result.lostToFees).toBeCloseTo(result.baselineValue, 6)
  })

  it('applique la commission de surperformance sur le gain annuel', () => {
    const result = simulateFees({ ...noFees, monthly: 0, years: 1, grossReturnPct: 10, performanceFeePct: 20 })
    expect(result.totalFees).toBeCloseTo(200, 6)
    expect(result.finalValue).toBeCloseTo(10_800, 6)
  })

  it('ne prélève pas de commission de surperformance sur une année en perte', () => {
    const result = simulateFees({ ...noFees, monthly: 0, years: 1, grossReturnPct: -5, performanceFeePct: 20 })
    expect(result.totalFees).toBe(0)
  })
})

describe('cas limites des frais', () => {
  it('sans versement, tout est nul', () => {
    const result = simulateFees({ ...noFees, initial: 0, monthly: 0, entryFeePct: 3, terPct: 2 })
    expect(result.finalValue).toBe(0)
    expect(result.totalFees).toBe(0)
    expect(result.lostToFees).toBe(0)
  })
  it('à horizon nul, seuls les frais sur le versement initial sont prélevés', () => {
    const result = simulateFees({ ...noFees, years: 0, entryFeePct: 2, managementFeePct: 1 })
    expect(result.rows).toHaveLength(1)
    expect(result.totalFees).toBeCloseTo(200, 8)
    expect(result.finalValue).toBeCloseTo(9800, 8)
  })
  it('un courtage fixe ne dépasse jamais le versement', () => {
    const result = simulateFees({
      ...noFees,
      initial: 0,
      monthly: 5,
      years: 1,
      grossReturnPct: 0,
      brokerage: { mode: 'fixed', value: 20 },
    })
    expect(result.finalValue).toBe(0)
    expect(result.totalFees).toBeCloseTo(60, 8)
  })
  it('enchaîne frais d’entrée puis courtage en pourcentage', () => {
    const result = simulateFees({
      ...noFees,
      monthly: 0,
      years: 0,
      entryFeePct: 10,
      brokerage: { mode: 'percent', value: 10 },
    })
    // 10 000 → 9 000 après entrée → 8 100 après courtage
    expect(result.finalValue).toBeCloseTo(8100, 8)
  })
  it('la valeur et les frais cumulés ne décroissent pas d’une année sur l’autre à rendement positif', () => {
    const { rows } = simulateFees({ ...noFees, entryFeePct: 1, managementFeePct: 0.7, terPct: 0.3 })
    for (let i = 1; i < rows.length; i++) {
      expect(rows[i].value).toBeGreaterThan(rows[i - 1].value)
      expect(rows[i].feesPaid).toBeGreaterThan(rows[i - 1].feesPaid)
      expect(rows[i].baseline).toBeGreaterThanOrEqual(rows[i].value)
    }
  })
  it('plus de frais courants, moins de valeur finale', () => {
    const cheap = simulateFees({ ...noFees, terPct: 0.2 })
    const costly = simulateFees({ ...noFees, terPct: 2 })
    expect(costly.finalValue).toBeLessThan(cheap.finalValue)
    expect(costly.baselineValue).toBeCloseTo(cheap.baselineValue, 6)
  })
})
