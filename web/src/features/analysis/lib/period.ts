// Période commune aux graphiques de l'analyse et unité d'affichage (euros ou pourcentage).
import type { HistoryRange } from '@/shared/api/types'
import { money, moneyCompact, pct } from '@/shared/lib/format'

export type PresetRange = HistoryRange
export type Period = { kind: 'preset'; range: PresetRange } | { kind: 'custom'; from: string; to: string }
export type Unit = 'eur' | 'pct'

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/

/** Période demandée à l'API : une période personnalisée se filtre dans l'historique complet. */
export function rangeToFetch(period: Period): HistoryRange {
  return period.kind === 'preset' ? period.range : 'all'
}

/** Message d'erreur d'une période personnalisée, ou null si elle est utilisable. */
export function periodError(period: Period): string | null {
  if (period.kind === 'preset') return null
  if (!ISO_DATE.test(period.from) || !ISO_DATE.test(period.to)) return 'Indiquez une date de début et une date de fin.'
  if (period.from > period.to) return 'La date de début doit précéder la date de fin.'
  return null
}

/** Points compris dans la période, bornes incluses. Une période invalide ne garde rien. */
export function withinPeriod<T extends { date: string }>(points: readonly T[], period: Period): T[] {
  if (period.kind === 'preset') return [...points]
  if (periodError(period)) return []
  return points.filter((point) => point.date >= period.from && point.date <= period.to)
}

/**
 * Lignes d'un tableau de données : toutes si elles sont peu nombreuses, sinon la dernière de chaque mois
 * (et toujours la toute dernière), pour qu'un historique de plusieurs années reste lisible.
 */
export function sampleForTable<T extends { date: string }>(rows: readonly T[], maxDaily = 31): T[] {
  if (rows.length <= maxDaily) return [...rows]
  return rows.filter((row, i) => i === rows.length - 1 || rows[i + 1].date.slice(0, 7) !== row.date.slice(0, 7))
}

/** Part de `value` dans `total`, en pourcentage ; null si le total est nul. */
export function share(value: number, total: number): number | null {
  return total !== 0 ? (value / Math.abs(total)) * 100 : null
}

/** Montant ou pourcentage selon l'unité ; `signed` préfixe les hausses d'un « + ». */
export function formatUnit(value: number | null | undefined, unit: Unit, signed = false): string {
  return unit === 'eur' ? money(value, { signed }) : pct(value, 1, signed)
}

/** Graduation compacte d'un axe de valeurs : `12 k€` ou `35 %`. */
export function formatAxis(value: number, unit: Unit): string {
  return unit === 'eur' ? moneyCompact(value) : pct(value, 0)
}
