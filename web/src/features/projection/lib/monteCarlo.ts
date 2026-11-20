// Projection par simulation de Monte-Carlo. Fonctions pures, sans dépendance au navigateur :
// exécutées dans un Web Worker à l'écran, appelées directement dans les tests.
//
// Modèle : chaque mois, le capital est multiplié par exp(X), X suivant une loi normale de moyenne
// ln(1 + r) / 12 et d'écart type σ / √12 (r = rendement annuel médian, σ = volatilité annuelle).
// À volatilité nulle, on retrouve exactement la capitalisation de `futureValue`. Versement en fin de mois ;
// le capital ne descend pas sous zéro (un retrait ne peut pas dépasser ce qui reste).

import { seededRandom, standardNormal } from './random'

export type MonteCarloInput = {
  initial: number
  monthly: number
  /** Rendement annuel médian, en pourcentage (5 = 5 %). */
  annualReturnPct: number
  /** Volatilité annuelle (écart type des rendements logarithmiques), en pourcentage. */
  volatilityPct: number
  years: number
  simulations: number
  seed: number
}

/** Percentiles du capital en fin d'année, en euros courants. */
export type FanRow = { year: number; p10: number; p50: number; p90: number }

export type MonteCarloResult = {
  rows: FanRow[]
  /** Capital final de chaque trajectoire, trié par ordre croissant. */
  finalValues: Float64Array
}

/**
 * Percentile `p` (entre 0 et 100) d'un échantillon déjà trié, par interpolation linéaire entre les rangs.
 * `NaN` pour un échantillon vide.
 */
export function percentile(sorted: ArrayLike<number>, p: number): number {
  const n = sorted.length
  if (n === 0) return Number.NaN
  const rank = (Math.min(100, Math.max(0, p)) / 100) * (n - 1)
  const low = Math.floor(rank)
  const high = Math.ceil(rank)
  return sorted[low] + (sorted[high] - sorted[low]) * (rank - low)
}

/**
 * Part des trajectoires dont le capital final atteint au moins `target` (entre 0 et 1).
 * `null` sans trajectoire. Recherche dichotomique sur l'échantillon trié.
 */
export function probabilityAtLeast(sorted: ArrayLike<number>, target: number): number | null {
  const n = sorted.length
  if (n === 0) return null
  let low = 0
  let high = n
  while (low < high) {
    const mid = (low + high) >>> 1
    if (sorted[mid] < target) low = mid + 1
    else high = mid
  }
  return (n - low) / n
}

/** Simule `simulations` trajectoires mensuelles et résume chaque année par ses percentiles 10, 50 et 90. */
export function simulateMonteCarlo(input: MonteCarloInput): MonteCarloResult {
  const years = Math.max(0, Math.floor(input.years))
  const paths = Math.max(1, Math.floor(input.simulations))
  const drift = Math.log(1 + input.annualReturnPct / 100) / 12
  const sigma = Math.max(0, input.volatilityPct) / 100 / Math.sqrt(12)
  const normal = standardNormal(seededRandom(input.seed))

  // byYear[y][k] : capital de la trajectoire k à la fin de l'année y.
  const byYear = Array.from({ length: years + 1 }, () => new Float64Array(paths))
  const start = Math.max(0, input.initial)
  for (let k = 0; k < paths; k++) {
    let value = start
    byYear[0][k] = value
    for (let year = 1; year <= years; year++) {
      for (let month = 0; month < 12; month++) {
        value = Math.max(0, value * Math.exp(drift + sigma * normal()) + input.monthly)
      }
      byYear[year][k] = value
    }
  }

  const rows = byYear.map((values, year) => {
    // Le tri d'un Float64Array est numérique.
    values.sort()
    return { year, p10: percentile(values, 10), p50: percentile(values, 50), p90: percentile(values, 90) }
  })
  return { rows, finalValues: byYear[years] }
}

/** Une année de l'éventail dans l'unité affichée, avec le total versé et l'année civile. */
export type FanPoint = FanRow & { contributed: number }

type Display = { initial: number; monthly: number; inflationPct: number; real: boolean; startYear: number }

/** Convertit les percentiles en euros d'aujourd'hui si `real`, et ajoute le total versé et l'année civile. */
export function toFanPoints(rows: FanRow[], display: Display): FanPoint[] {
  return rows.map((row) => {
    const deflator = display.real ? Math.pow(1 + display.inflationPct / 100, row.year) : 1
    return {
      year: display.startYear + row.year,
      p10: row.p10 / deflator,
      p50: row.p50 / deflator,
      p90: row.p90 / deflator,
      contributed: Math.max(0, display.initial) + display.monthly * 12 * row.year,
    }
  })
}

/**
 * Probabilité d'atteindre `target` à la fin de la simulation. Une cible exprimée en euros d'aujourd'hui (`real`)
 * est d'abord portée en euros courants avec l'inflation sur la durée simulée.
 */
export function reachProbability(
  result: MonteCarloResult,
  target: number,
  options: { inflationPct: number; real: boolean },
): number | null {
  const years = result.rows.length - 1
  const nominalTarget = options.real ? target * Math.pow(1 + options.inflationPct / 100, years) : target
  return probabilityAtLeast(result.finalValues, nominalTarget)
}
