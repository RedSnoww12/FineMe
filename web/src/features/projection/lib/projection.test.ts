import { describe, expect, it } from 'vitest'
import { futureValue } from '@/shared/lib/compound'
import { fireTarget, monthsToReach, projectGrowth, toRealValue } from './projection'

describe('projectGrowth', () => {
  const rows = projectGrowth({ initial: 10_000, monthly: 500, annualReturnPct: 6, inflationPct: 2, years: 10 })
  it('produit une ligne par année, année 0 incluse', () => {
    expect(rows).toHaveLength(11)
    expect(rows[0]).toEqual({ year: 0, contributed: 10_000, nominal: 10_000, real: 10_000 })
  })
  it('cumule les versements', () => {
    expect(rows[10].contributed).toBe(10_000 + 500 * 120)
  })
  it('corrige de l’inflation', () => {
    expect(rows[10].real).toBeCloseTo(rows[10].nominal / Math.pow(1.02, 10), 6)
    expect(toRealValue(1020, 2, 1)).toBeCloseTo(1000, 8)
  })
})

describe('fireTarget', () => {
  it('applique la règle des 4 %', () => {
    expect(fireTarget(30_000, 4)).toBeCloseTo(750_000, 6)
  })
  it('refuse un taux de retrait nul ou négatif', () => {
    expect(fireTarget(30_000, 0)).toBeNull()
    expect(fireTarget(30_000, -1)).toBeNull()
  })
})

describe('monthsToReach', () => {
  it('renvoie 0 quand la cible est déjà atteinte', () => {
    expect(monthsToReach({ initial: 1000, monthly: 0, annualReturnPct: 5, inflationPct: 0, target: 900 })).toBe(0)
  })
  it('compte les mois à taux nul', () => {
    expect(monthsToReach({ initial: 0, monthly: 100, annualReturnPct: 0, inflationPct: 0, target: 1200 })).toBe(12)
  })
  it('est cohérent avec futureValue', () => {
    const months = monthsToReach({ initial: 20_000, monthly: 800, annualReturnPct: 6, inflationPct: 0, target: 300_000 })
    expect(months).not.toBeNull()
    expect(futureValue(20_000, 800, 6, months as number)).toBeGreaterThanOrEqual(300_000)
    expect(futureValue(20_000, 800, 6, (months as number) - 1)).toBeLessThan(300_000)
  })
  it('met plus de temps avec de l’inflation', () => {
    const base = { initial: 20_000, monthly: 800, annualReturnPct: 6, target: 300_000 }
    const without = monthsToReach({ ...base, inflationPct: 0 }) as number
    const withInflation = monthsToReach({ ...base, inflationPct: 2 }) as number
    expect(withInflation).toBeGreaterThan(without)
  })
  it('renvoie null quand la cible est hors de portée', () => {
    expect(
      monthsToReach({ initial: 0, monthly: 10, annualReturnPct: 0, inflationPct: 0, target: 1_000_000, maxYears: 50 }),
    ).toBeNull()
  })
})


describe('cas limites de la projection', () => {
  it('gère un horizon nul ou négatif', () => {
    expect(projectGrowth({ initial: 500, monthly: 50, annualReturnPct: 5, inflationPct: 2, years: 0 })).toHaveLength(1)
    expect(projectGrowth({ initial: 500, monthly: 50, annualReturnPct: 5, inflationPct: 2, years: -3 })).toHaveLength(1)
  })
  it('tronque un horizon fractionnaire', () => {
    expect(projectGrowth({ initial: 0, monthly: 100, annualReturnPct: 0, inflationPct: 0, years: 2.9 })).toHaveLength(3)
  })
  it('fait décroître le capital avec un rendement négatif', () => {
    const rows = projectGrowth({ initial: 10_000, monthly: 0, annualReturnPct: -10, inflationPct: 0, years: 2 })
    expect(rows[2].nominal).toBeCloseTo(8100, 6)
  })
  it('sans inflation, valeur réelle et nominale coïncident', () => {
    const rows = projectGrowth({ initial: 1000, monthly: 100, annualReturnPct: 4, inflationPct: 0, years: 5 })
    expect(rows.every((row) => row.real === row.nominal)).toBe(true)
  })
  it('avec un rendement égal à l’inflation, le capital seul garde son pouvoir d’achat', () => {
    const rows = projectGrowth({ initial: 1000, monthly: 0, annualReturnPct: 3, inflationPct: 3, years: 15 })
    expect(rows[15].real).toBeCloseTo(1000, 6)
  })
  it('une cible nulle est atteinte tout de suite', () => {
    expect(monthsToReach({ initial: 0, monthly: 0, annualReturnPct: 0, inflationPct: 0, target: 0 })).toBe(0)
  })
  it('sans épargne ni rendement, une cible supérieure au capital est hors de portée', () => {
    expect(monthsToReach({ initial: 100, monthly: 0, annualReturnPct: 0, inflationPct: 0, target: 101 })).toBeNull()
  })
  it('un rendement inférieur à l’inflation peut rendre la cible inatteignable', () => {
    expect(
      monthsToReach({ initial: 10_000, monthly: 0, annualReturnPct: 1, inflationPct: 3, target: 20_000 }),
    ).toBeNull()
  })
  it('des dépenses nulles donnent une cible nulle, des dépenses négatives aucune', () => {
    expect(fireTarget(0, 4)).toBe(0)
    expect(fireTarget(-1, 4)).toBeNull()
  })
})
