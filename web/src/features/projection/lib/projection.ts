import { futureValue, monthlyRate } from '@/shared/lib/compound'

/** Convertit une valeur future en euros d'aujourd'hui. */
export function toRealValue(nominal: number, inflationPct: number, years: number): number {
  return nominal / Math.pow(1 + inflationPct / 100, years)
}

export type ProjectionInput = {
  initial: number
  monthly: number
  annualReturnPct: number
  inflationPct: number
  years: number
}

export type ProjectionRow = {
  year: number
  /** Capital initial + versements cumulés. */
  contributed: number
  nominal: number
  /** Valeur en euros d'aujourd'hui. */
  real: number
}

/** Une ligne par année, de 0 (aujourd'hui) à `years`. */
export function projectGrowth(input: ProjectionInput): ProjectionRow[] {
  const years = Math.max(0, Math.floor(input.years))
  const rows: ProjectionRow[] = []
  for (let year = 0; year <= years; year++) {
    const nominal = futureValue(input.initial, input.monthly, input.annualReturnPct, year * 12)
    rows.push({
      year,
      contributed: input.initial + input.monthly * year * 12,
      nominal,
      real: toRealValue(nominal, input.inflationPct, year),
    })
  }
  return rows
}

/** Capital nécessaire pour financer `annualExpenses` avec un taux de retrait donné. */
export function fireTarget(annualExpenses: number, withdrawalRatePct: number): number | null {
  if (withdrawalRatePct <= 0 || annualExpenses < 0) return null
  return annualExpenses / (withdrawalRatePct / 100)
}

/**
 * Nombre de mois avant que le capital, exprimé en euros d'aujourd'hui, atteigne `target`.
 * `null` si la cible n'est pas atteinte avant `maxYears`.
 */
export function monthsToReach(
  input: Omit<ProjectionInput, 'years'> & { target: number; maxYears?: number },
): number | null {
  const maxMonths = (input.maxYears ?? 80) * 12
  const r = monthlyRate(input.annualReturnPct)
  const i = monthlyRate(input.inflationPct)
  let value = input.initial
  let deflator = 1
  for (let month = 0; month <= maxMonths; month++) {
    if (value / deflator >= input.target) return month
    value = value * (1 + r) + input.monthly
    deflator *= 1 + i
  }
  return null
}

