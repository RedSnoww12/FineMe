// Immobilier dans le temps : capital remboursé et restant dû des prêts liés, valeur nette des biens.
import type { RealEstateItem, RealEstateLoan, ScheduleRow, Valuation } from '@/shared/api/types'
import { valuationOn } from '@/shared/lib/valuation'
import { share, type Unit } from './period'

/** Tableau d'amortissement d'un prêt et part de ce prêt détenue par la vue. */
export type LoanTimeline = { loan: RealEstateLoan; fraction: number; rows: readonly ScheduleRow[] }

export type EquityRow = {
  /** Premier jour du mois, `AAAA-MM-01`. */
  date: string
  value: number | null
  remaining: number | null
  repaid: number | null
  equity: number | null
}

function monthOf(isoDate: string): string {
  return isoDate.slice(0, 7)
}

function addMonths(month: string, count: number): string {
  const [year, m] = month.split('-').map(Number)
  const index = year * 12 + (m - 1) + count
  return `${Math.floor(index / 12)}-${String((index % 12) + 1).padStart(2, '0')}`
}

/** Capital restant dû d'un prêt à la fin d'un mois : le capital emprunté avant la première échéance, 0 après la dernière. */
function remainingAt(timeline: LoanTimeline, month: string, byMonth: ReadonlyMap<string, number>): number {
  const first = timeline.rows[0]
  const last = timeline.rows[timeline.rows.length - 1]
  if (!first || !last) return timeline.loan.remaining_capital
  if (month < monthOf(first.date)) return timeline.loan.principal
  if (month > monthOf(last.date)) return 0
  return byMonth.get(month) ?? timeline.loan.remaining_capital
}

/**
 * Valeur détenue des biens à la fin d'un mois : estimation en vigueur de chaque bien, convertie et pondérée comme
 * sa valeur actuelle détenue ; un bien sans estimation à cette date compte pour sa valeur actuelle.
 */
function valueAt(items: readonly RealEstateItem[], valuations: ReadonlyMap<string, readonly Valuation[]>, month: string): number {
  return items.reduce((sum, item) => {
    const estimate = valuationOn(valuations.get(item.account.id) ?? [], `${month}-31`)
    const current = item.property.current_value
    if (!estimate || current <= 0) return sum + item.account.owned_value
    return sum + (estimate.value * item.account.owned_value) / current
  }, 0)
}

/**
 * Série mensuelle, du mois précédant la première échéance à la dernière échéance des prêts liés aux biens.
 * La valeur des biens suit leurs estimations datées (la plus récente au-delà, la valeur actuelle faute
 * d'estimation) ; les montants des prêts sont pondérés par la part détenue de chaque prêt. En pourcentage,
 * tout est rapporté à la valeur des biens du mois.
 */
export function equitySeries(
  items: readonly RealEstateItem[],
  timelines: readonly LoanTimeline[],
  unit: Unit,
  valuations: ReadonlyMap<string, readonly Valuation[]> = new Map(),
): EquityRow[] {
  const active = timelines.filter((timeline) => timeline.fraction > 0 && timeline.rows.length > 0)
  if (active.length === 0) return []

  const months = active.flatMap((timeline) => [monthOf(timeline.rows[0].date), monthOf(timeline.rows[timeline.rows.length - 1].date)])
  const start = addMonths(months.reduce((a, b) => (a < b ? a : b)), -1)
  const end = months.reduce((a, b) => (a > b ? a : b))
  const indexed = active.map((timeline) => ({
    timeline,
    byMonth: new Map(timeline.rows.map((row) => [monthOf(row.date), row.remaining])),
  }))

  const rows: EquityRow[] = []
  for (let month = start; month <= end; month = addMonths(month, 1)) {
    const value = valueAt(items, valuations, month)
    const scale = (amount: number) => (unit === 'eur' ? amount : share(amount, value))
    let remaining = 0
    let repaid = 0
    for (const { timeline, byMonth } of indexed) {
      const left = remainingAt(timeline, month, byMonth)
      remaining += left * timeline.fraction
      repaid += (timeline.loan.principal - left) * timeline.fraction
    }
    rows.push({
      date: `${month}-01`,
      value: scale(value),
      remaining: scale(remaining),
      repaid: scale(repaid),
      equity: scale(value - remaining),
    })
  }
  return rows
}

/** Une ligne par an (janvier) et la dernière, pour l'alternative tabulaire d'un prêt sur 20 ou 25 ans. */
export function yearlyRows(rows: readonly EquityRow[]): EquityRow[] {
  return rows.filter((row, i) => i === 0 || i === rows.length - 1 || row.date.slice(5, 7) === '01')
}
