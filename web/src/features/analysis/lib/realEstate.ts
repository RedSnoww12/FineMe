// Immobilier dans le temps : capital remboursé et restant dû des prêts liés, valeur nette des biens.
import type { RealEstateItem, RealEstateLoan, ScheduleRow } from '@/shared/api/types'
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
 * Série mensuelle, du mois précédant la première échéance à la dernière échéance des prêts liés aux biens.
 * La valeur des biens est leur valeur actuelle détenue (supposée constante) ; les montants des prêts sont
 * pondérés par la part détenue de chaque prêt. En pourcentage, tout est rapporté à la valeur des biens.
 */
export function equitySeries(items: readonly RealEstateItem[], timelines: readonly LoanTimeline[], unit: Unit): EquityRow[] {
  const value = items.reduce((sum, item) => sum + item.account.owned_value, 0)
  const active = timelines.filter((timeline) => timeline.fraction > 0 && timeline.rows.length > 0)
  if (active.length === 0) return []

  const months = active.flatMap((timeline) => [monthOf(timeline.rows[0].date), monthOf(timeline.rows[timeline.rows.length - 1].date)])
  const start = addMonths(months.reduce((a, b) => (a < b ? a : b)), -1)
  const end = months.reduce((a, b) => (a > b ? a : b))
  const indexed = active.map((timeline) => ({
    timeline,
    byMonth: new Map(timeline.rows.map((row) => [monthOf(row.date), row.remaining])),
  }))

  const scale = (amount: number) => (unit === 'eur' ? amount : share(amount, value))
  const rows: EquityRow[] = []
  for (let month = start; month <= end; month = addMonths(month, 1)) {
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
