import { describe, expect, it } from 'vitest'
import { futureValue } from '@/shared/lib/compound'
import {
  percentile,
  probabilityAtLeast,
  reachProbability,
  simulateMonteCarlo,
  toFanPoints,
  type MonteCarloInput,
} from './monteCarlo'

const base: MonteCarloInput = {
  initial: 10_000,
  monthly: 300,
  annualReturnPct: 5,
  volatilityPct: 15,
  years: 20,
  simulations: 1000,
  seed: 1,
}

describe('percentile', () => {
  const sorted = [10, 20, 30, 40, 50]
  it('renvoie les extrêmes et la médiane', () => {
    expect(percentile(sorted, 0)).toBe(10)
    expect(percentile(sorted, 50)).toBe(30)
    expect(percentile(sorted, 100)).toBe(50)
  })
  it('interpole entre deux rangs', () => {
    expect(percentile(sorted, 10)).toBeCloseTo(14, 10)
    expect(percentile([0, 100], 90)).toBeCloseTo(90, 10)
  })
  it('borne le percentile demandé', () => {
    expect(percentile(sorted, -5)).toBe(10)
    expect(percentile(sorted, 150)).toBe(50)
  })
  it('gère un seul élément et un échantillon vide', () => {
    expect(percentile([7], 90)).toBe(7)
    expect(percentile([], 50)).toBeNaN()
  })
})

describe('probabilityAtLeast', () => {
  const sorted = new Float64Array([100, 200, 200, 300])
  it('compte les valeurs supérieures ou égales à la cible', () => {
    expect(probabilityAtLeast(sorted, 200)).toBe(0.75)
    expect(probabilityAtLeast(sorted, 201)).toBe(0.25)
  })
  it('vaut 1 pour une cible déjà dépassée par toutes les trajectoires', () => {
    expect(probabilityAtLeast(sorted, 0)).toBe(1)
    expect(probabilityAtLeast(sorted, 100)).toBe(1)
  })
  it('vaut 0 pour une cible hors de portée', () => {
    expect(probabilityAtLeast(sorted, 300.01)).toBe(0)
  })
  it('renvoie null sans trajectoire', () => {
    expect(probabilityAtLeast([], 100)).toBeNull()
  })
})

describe('simulateMonteCarlo', () => {
  it('est reproductible avec la même graine', () => {
    const a = simulateMonteCarlo(base)
    const b = simulateMonteCarlo(base)
    expect(a.rows).toEqual(b.rows)
    expect(Array.from(a.finalValues)).toEqual(Array.from(b.finalValues))
  })
  it('change avec une autre graine', () => {
    expect(simulateMonteCarlo({ ...base, seed: 2 }).rows).not.toEqual(simulateMonteCarlo(base).rows)
  })
  it('produit une ligne par année, année 0 au capital de départ', () => {
    const { rows, finalValues } = simulateMonteCarlo(base)
    expect(rows).toHaveLength(21)
    expect(rows[0]).toEqual({ year: 0, p10: 10_000, p50: 10_000, p90: 10_000 })
    expect(finalValues).toHaveLength(1000)
  })
  it('ordonne les percentiles et trie les capitaux finaux', () => {
    const { rows, finalValues } = simulateMonteCarlo(base)
    for (const row of rows) {
      expect(row.p10).toBeLessThanOrEqual(row.p50)
      expect(row.p50).toBeLessThanOrEqual(row.p90)
    }
    for (let i = 1; i < finalValues.length; i++) expect(finalValues[i]).toBeGreaterThanOrEqual(finalValues[i - 1])
    expect(rows[20].p90 - rows[20].p10).toBeGreaterThan(rows[5].p90 - rows[5].p10)
  })
  it('retrouve la capitalisation déterministe à volatilité nulle', () => {
    const { rows } = simulateMonteCarlo({ ...base, volatilityPct: 0, simulations: 3 })
    const expected = futureValue(10_000, 300, 5, 240)
    expect(rows[20].p10).toBeCloseTo(expected, 4)
    expect(rows[20].p90).toBeCloseTo(expected, 4)
  })
  it('a pour médiane la capitalisation au rendement attendu, sans versement', () => {
    const { rows } = simulateMonteCarlo({ ...base, monthly: 0, simulations: 4000 })
    const deterministic = futureValue(10_000, 0, 5, 240)
    expect(Math.abs(rows[20].p50 / deterministic - 1)).toBeLessThan(0.05)
  })
  it('ne descend pas sous zéro avec des retraits', () => {
    const { rows } = simulateMonteCarlo({ ...base, initial: 1000, monthly: -500, years: 3 })
    expect(rows[3].p90).toBe(0)
  })
  it('gère un horizon nul et un nombre de simulations invalide', () => {
    const { rows, finalValues } = simulateMonteCarlo({ ...base, years: 0, simulations: 0 })
    expect(rows).toEqual([{ year: 0, p10: 10_000, p50: 10_000, p90: 10_000 }])
    expect(finalValues).toHaveLength(1)
  })
})

describe('toFanPoints', () => {
  const rows = [
    { year: 0, p10: 100, p50: 100, p90: 100 },
    { year: 1, p10: 102, p50: 204, p90: 306 },
  ]
  it('garde les euros courants et ajoute le total versé et l’année civile', () => {
    expect(toFanPoints(rows, { initial: 100, monthly: 10, inflationPct: 2, real: false, startYear: 2030 })).toEqual([
      { year: 2030, p10: 100, p50: 100, p90: 100, contributed: 100 },
      { year: 2031, p10: 102, p50: 204, p90: 306, contributed: 220 },
    ])
  })
  it('corrige de l’inflation en euros d’aujourd’hui', () => {
    const [, point] = toFanPoints(rows, { initial: 100, monthly: 0, inflationPct: 2, real: true, startYear: 2030 })
    expect(point.p10).toBeCloseTo(100, 10)
    expect(point.p50).toBeCloseTo(200, 10)
    expect(point.p90).toBeCloseTo(300, 10)
  })
})

describe('reachProbability', () => {
  const result = { rows: [{ year: 0, p10: 0, p50: 0, p90: 0 }, { year: 1, p10: 0, p50: 0, p90: 0 }], finalValues: new Float64Array([100, 125]) }
  it('compare une cible en euros courants directement', () => {
    expect(reachProbability(result, 105, { inflationPct: 25, real: false })).toBe(0.5)
  })
  it('porte une cible en euros d’aujourd’hui en euros courants', () => {
    // 100 € d'aujourd'hui valent 125 € dans un an avec 25 % d'inflation.
    expect(reachProbability(result, 100, { inflationPct: 25, real: true })).toBe(0.5)
    expect(reachProbability(result, 95, { inflationPct: 0, real: true })).toBe(1)
  })
})
