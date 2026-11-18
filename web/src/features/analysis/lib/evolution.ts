// Séries d'évolution : actifs par catégorie (aires empilées) et actifs, dettes, patrimoine net (courbes).
import type { AllocationCategory } from '@/shared/api/types'
import type { DailyHistoryPoint } from '@/shared/lib/history'
import { share, type Unit } from './period'

/** Ordre d'empilement : du plus stable (bas) au plus volatil (haut). */
export const ASSET_CATEGORIES: AllocationCategory[] = ['real_estate', 'checking', 'savings', 'other', 'investment', 'crypto']

export type CategoryRow = Record<AllocationCategory, number | null> & {
  date: string
  estimated: boolean
}

function emptyCategories(value: number | null): Record<AllocationCategory, number | null> {
  return { checking: value, savings: value, investment: value, crypto: value, real_estate: value, other: value }
}

/**
 * Valeur des actifs par catégorie, jour par jour. En pourcentage, chaque catégorie est rapportée aux actifs
 * bruts du jour (les aires empilées font 100 %). Un jour sans détail par compte n'a aucune valeur (trou).
 */
export function categoryRows(series: readonly DailyHistoryPoint[], unit: Unit): CategoryRow[] {
  return series.map((point) => {
    if (point.accounts === null) return { date: point.date, estimated: point.estimated, ...emptyCategories(null) }
    const values = emptyCategories(0)
    for (const account of point.accounts) {
      if (account.kind === 'loan') continue
      values[account.kind] = (values[account.kind] ?? 0) + account.value
    }
    if (unit === 'pct') {
      const total = ASSET_CATEGORIES.reduce((sum, category) => sum + (values[category] ?? 0), 0)
      for (const category of ASSET_CATEGORIES) values[category] = share(values[category] ?? 0, total) ?? 0
    }
    return { date: point.date, estimated: point.estimated, ...values }
  })
}

/** Catégories présentes au moins un jour, dans l'ordre d'empilement. */
export function presentCategories(rows: readonly CategoryRow[]): AllocationCategory[] {
  return ASSET_CATEGORIES.filter((category) => rows.some((row) => (row[category] ?? 0) !== 0))
}

/** Nombre de jours de la série sans détail par compte (instantanés antérieurs au détail). */
export function countUndetailedDays(series: readonly DailyHistoryPoint[]): number {
  return series.filter((point) => point.accounts === null).length
}

export type BalanceRow = {
  date: string
  estimated: boolean
  gross_assets: number | null
  liabilities: number | null
  net_worth: number | null
}

/**
 * Actifs bruts, dettes et patrimoine net. En pourcentage, chaque série devient sa variation depuis le
 * premier jour de la période (null si elle partait de zéro : aucune variation relative n'a de sens).
 */
export function balanceRows(series: readonly DailyHistoryPoint[], unit: Unit): BalanceRow[] {
  const first = series[0]
  return series.map((point) => {
    if (unit === 'eur' || !first) {
      return {
        date: point.date,
        estimated: point.estimated,
        gross_assets: point.gross_assets,
        liabilities: point.liabilities,
        net_worth: point.net_worth,
      }
    }
    const change = (value: number, start: number) => share(value - start, start)
    return {
      date: point.date,
      estimated: point.estimated,
      gross_assets: change(point.gross_assets, first.gross_assets),
      liabilities: change(point.liabilities, first.liabilities),
      net_worth: change(point.net_worth, first.net_worth),
    }
  })
}
