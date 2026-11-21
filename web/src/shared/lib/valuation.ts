// Estimations datées d'un bien : valeur à une date, estimation au prix du m², courbe valeur / valeur nette.
import type { Loan, ScheduleRow, Valuation } from '@/shared/api/types'

/** Plus récente d'abord ; à date égale, la dernière saisie. */
function newer(a: Valuation, b: Valuation): number {
  return b.valued_on.localeCompare(a.valued_on) || b.recorded_at.localeCompare(a.recorded_at)
}

/** Estimation en vigueur à `isoDate` (la plus récente datée au plus tard ce jour), ou `null`. */
export function valuationOn(valuations: readonly Valuation[], isoDate: string): Valuation | null {
  return [...valuations].filter((v) => v.valued_on <= isoDate).sort(newer)[0] ?? null
}

export function newestFirst(valuations: readonly Valuation[]): Valuation[] {
  return [...valuations].sort(newer)
}

/** Valeur d'un bien au prix du m² : surface × prix, `null` si l'un manque ou n'est pas strictement positif. */
export function valueFromPricePerM2(surface: number | null | undefined, pricePerM2: number | null | undefined): number | null {
  if (!surface || !pricePerM2 || surface <= 0 || pricePerM2 <= 0) return null
  return surface * pricePerM2
}

/** Tableau d'amortissement d'un prêt lié au bien. */
export type LoanRows = { loan: Pick<Loan, 'principal'>; rows: readonly ScheduleRow[] }

/**
 * Capital restant dû d'un prêt à une date : le capital emprunté avant la première échéance,
 * celui de la dernière échéance passée ensuite (0 après la dernière).
 */
export function remainingOn({ loan, rows }: LoanRows, isoDate: string): number {
  let remaining = loan.principal
  for (const row of rows) {
    if (row.date > isoDate) break
    remaining = row.remaining
  }
  return remaining
}

export type ValuationPoint = { date: string; value: number; debt: number; net: number }

/**
 * Une ligne par date d'estimation, plus aujourd'hui : valeur en vigueur, dette des prêts liés à cette date
 * et valeur nette (valeur − capital restant dû). Montants à 100 % du bien.
 */
export function valuationSeries(valuations: readonly Valuation[], loans: readonly LoanRows[], today: string): ValuationPoint[] {
  const dates = [...new Set([...valuations.map((v) => v.valued_on).filter((d) => d <= today), today])].sort()
  const points: ValuationPoint[] = []
  for (const date of dates) {
    const current = valuationOn(valuations, date)
    if (!current) continue
    const debt = loans.reduce((sum, loan) => sum + remainingOn(loan, date), 0)
    points.push({ date, value: current.value, debt, net: current.value - debt })
  }
  return points
}
