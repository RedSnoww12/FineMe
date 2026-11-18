// Série quotidienne du patrimoine : un point par jour, les jours sans instantané étant estimés.
import type { History, HistoryAccount, HistoryPoint } from '@/shared/api/types'

/** Point d'une série quotidienne. `estimated` : aucun instantané ce jour-là, valeurs interpolées. */
export type DailyHistoryPoint = HistoryPoint & { estimated: boolean }

/** Point tel que renvoyé par une API antérieure au détail par compte : le champ `accounts` peut manquer. */
type RawHistoryPoint = Omit<HistoryPoint, 'accounts'> & { accounts?: HistoryAccount[] | null }

/**
 * Ramène une réponse de `/api/history` au contrat : un `accounts` absent (API pas encore à jour) devient
 * `null`, comme un instantané enregistré avant le détail par compte.
 */
export function normalizeHistory(raw: { points: readonly RawHistoryPoint[] }): History {
  return { points: raw.points.map((point) => ({ ...point, accounts: point.accounts ?? null })) }
}

const DAY_MS = 86_400_000
const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/

function dayNumber(isoDate: string): number | null {
  const match = ISO_DATE.exec(isoDate)
  if (!match) return null
  return Math.round(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])) / DAY_MS)
}

function isoDateOf(day: number): string {
  return new Date(day * DAY_MS).toISOString().slice(0, 10)
}

function lerp(from: number, to: number, ratio: number): number {
  return from + (to - from) * ratio
}

function lerpGain(from: number | null, to: number | null, ratio: number): number | null {
  if (from === null && to === null) return null
  return lerp(from ?? 0, to ?? 0, ratio)
}

/**
 * Détail par compte d'un jour estimé : chaque compte est interpolé ; un compte présent d'un seul côté
 * (ouvert ou clôturé entre les deux relevés) vaut 0 de l'autre. Sans détail d'un côté, pas de détail.
 */
function interpolateAccounts(
  before: HistoryAccount[] | null,
  after: HistoryAccount[] | null,
  ratio: number,
): HistoryAccount[] | null {
  if (before === null || after === null) return null
  const afterById = new Map(after.map((account) => [account.account_id, account]))
  const beforeIds = new Set(before.map((account) => account.account_id))
  const fromBefore = before.map((account) => {
    const next = afterById.get(account.account_id)
    return {
      ...account,
      value: lerp(account.value, next?.value ?? 0, ratio),
      gain: lerpGain(account.gain, next?.gain ?? null, ratio),
    }
  })
  const opened = after
    .filter((account) => !beforeIds.has(account.account_id))
    .map((account) => ({ ...account, value: lerp(0, account.value, ratio), gain: lerpGain(null, account.gain, ratio) }))
  return [...fromBefore, ...opened]
}

function interpolate(before: HistoryPoint, after: HistoryPoint, ratio: number, date: string): DailyHistoryPoint {
  return {
    date,
    gross_assets: lerp(before.gross_assets, after.gross_assets, ratio),
    liabilities: lerp(before.liabilities, after.liabilities, ratio),
    net_worth: lerp(before.net_worth, after.net_worth, ratio),
    accounts: interpolateAccounts(before.accounts, after.accounts, ratio),
    estimated: true,
  }
}

/**
 * Complète l'historique pour obtenir un point par jour, du premier au dernier instantané.
 * Les jours manquants (API en veille, tâche quotidienne en échec) sont interpolés linéairement entre
 * les deux instantanés qui les encadrent et marqués `estimated`. Rien n'est extrapolé avant le premier
 * ni après le dernier instantané ; un point dont la date est illisible est ignoré.
 */
export function fillMissingDays(points: readonly HistoryPoint[]): DailyHistoryPoint[] {
  const dated = points
    .map((point) => ({ point, day: dayNumber(point.date) }))
    .filter((entry): entry is { point: HistoryPoint; day: number } => entry.day !== null)
    .sort((a, b) => a.day - b.day)

  const series: DailyHistoryPoint[] = []
  let previous: { point: HistoryPoint; day: number } | null = null
  for (const current of dated) {
    if (previous && previous.day === current.day) continue // doublon : on garde le premier
    if (previous) {
      const gap = current.day - previous.day
      for (let missing = previous.day + 1; missing < current.day; missing++) {
        const ratio = (missing - previous.day) / gap
        series.push(interpolate(previous.point, current.point, ratio, isoDateOf(missing)))
      }
    }
    series.push({ ...current.point, estimated: false })
    previous = current
  }
  return series
}

/** Nombre de jours estimés dans une série quotidienne. */
export function countEstimatedDays(series: readonly DailyHistoryPoint[]): number {
  return series.filter((point) => point.estimated).length
}

/** Ligne de graphique : la valeur relevée et la valeur estimée sont deux séries, tracées différemment. */
export type HistoryChartRow = DailyHistoryPoint & { measured: number | null; estimate: number | null }

/**
 * Sépare une série quotidienne en deux tracés : `measured` porte les jours relevés, `estimate` les jours
 * estimés plus les instantanés qui les encadrent, pour que le tracé estimé rejoigne le tracé relevé.
 */
export function toHistoryChartRows(series: readonly DailyHistoryPoint[]): HistoryChartRow[] {
  return series.map((point, i) => {
    const touchesEstimate = point.estimated || series[i - 1]?.estimated === true || series[i + 1]?.estimated === true
    return {
      ...point,
      measured: point.estimated ? null : point.net_worth,
      estimate: touchesEstimate ? point.net_worth : null,
    }
  })
}
