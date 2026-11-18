// Objectifs : trajectoire réelle (historique des comptes liés) face à la trajectoire prévue.
import type { Goal } from '@/shared/api/types'
import { futureValue } from '@/shared/lib/compound'
import type { DailyHistoryPoint } from '@/shared/lib/history'
import { share, type Unit } from './period'

export type GoalPoint = { date: string; amount: number }

/**
 * Montant atteint jour après jour : valeur détenue par le foyer des comptes liés, hors dettes, comme
 * l'avancement calculé par l'API. Les jours sans détail par compte sont ignorés.
 */
export function goalRealSeries(goal: Goal, series: readonly DailyHistoryPoint[]): GoalPoint[] {
  const linked = new Set(goal.account_ids)
  return series.flatMap((point) => {
    if (point.accounts === null) return []
    const amount = point.accounts
      .filter((account) => linked.has(account.account_id) && account.kind !== 'loan')
      .reduce((sum, account) => sum + account.value, 0)
    return [{ date: point.date, amount }]
  })
}

function monthsBetween(from: string, to: string): number {
  const [fy, fm, fd] = from.split('-').map(Number)
  const [ty, tm, td] = to.split('-').map(Number)
  return (ty - fy) * 12 + (tm - fm) + (td >= fd ? 0 : -1)
}

function addMonths(isoDate: string, count: number): string {
  const [year, month, day] = isoDate.split('-').map(Number)
  const index = year * 12 + (month - 1) + count
  const y = Math.floor(index / 12)
  const m = (index % 12) + 1
  const lastDay = new Date(Date.UTC(y, m, 0)).getUTCDate()
  return `${y}-${String(m).padStart(2, '0')}-${String(Math.min(day, lastDay)).padStart(2, '0')}`
}

/**
 * Trajectoire prévue d'un projet daté : depuis `start`, versement mensuel et rendement annuel attendus de
 * l'objectif (0 s'ils ne sont pas renseignés), mois par mois jusqu'à l'échéance incluse, en mois entiers
 * comme `projected_amount_at_date`. Vide pour une
 * épargne de précaution, un projet sans échéance ou une échéance déjà passée.
 */
export function goalPlannedSeries(goal: Goal, start: GoalPoint): GoalPoint[] {
  if (goal.kind !== 'project' || !goal.target_date || goal.target_date <= start.date) return []
  const monthly = goal.monthly_contribution ?? 0
  const rate = goal.expected_return_pct ?? 0
  const months = monthsBetween(start.date, goal.target_date)
  const points: GoalPoint[] = []
  for (let m = 0; m <= months; m++) {
    points.push({ date: addMonths(start.date, m), amount: futureValue(start.amount, monthly, rate, m) })
  }
  // Échéance en cours de mois : pas de versement supplémentaire, comme le calcul de l'API (mois entiers).
  if (points[points.length - 1].date !== goal.target_date) {
    points.push({ date: goal.target_date, amount: futureValue(start.amount, monthly, rate, months) })
  }
  return points
}

export type GoalRow = { date: string; real: number | null; planned: number | null }

/**
 * Lignes du graphique, triées par date : chaque date porte la valeur réelle, la valeur prévue, ou les deux.
 * En pourcentage, les montants sont rapportés à la cible de l'objectif.
 */
export function goalChartRows(
  real: readonly GoalPoint[],
  planned: readonly GoalPoint[],
  target: number,
  unit: Unit,
): GoalRow[] {
  const scale = (amount: number) => (unit === 'eur' ? amount : share(amount, target))
  const byDate = new Map<string, GoalRow>()
  const at = (date: string) => {
    const existing = byDate.get(date)
    if (existing) return existing
    const created: GoalRow = { date, real: null, planned: null }
    byDate.set(date, created)
    return created
  }
  for (const point of real) at(point.date).real = scale(point.amount)
  for (const point of planned) at(point.date).planned = scale(point.amount)
  return [...byDate.values()].sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0))
}

/** Écart entre le montant atteint et le montant prévu au même jour (dernier jour réel), ou null. */
export function gapToPlan(real: readonly GoalPoint[], planned: readonly GoalPoint[]): number | null {
  const last = real[real.length - 1]
  if (!last || planned.length === 0) return null
  const before = [...planned].reverse().find((point) => point.date <= last.date)
  const after = planned.find((point) => point.date >= last.date)
  if (!before || !after) return null
  if (before.date === after.date) return last.amount - before.amount
  const span = Date.parse(after.date) - Date.parse(before.date)
  const ratio = (Date.parse(last.date) - Date.parse(before.date)) / span
  return last.amount - (before.amount + (after.amount - before.amount) * ratio)
}
