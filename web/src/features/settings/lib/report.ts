import type { Account, AccountKind, HistoryPoint } from '@/shared/api/types'
import { ACCOUNT_KIND_ORDER } from '@/shared/lib/labels'
import { viewShare } from './tabular'

/** `Camille Dupré` → `camille-dupre` : pour un nom de fichier sans accent ni espace. */
export function slug(text: string): string {
  return (
    text
      .normalize('NFD')
      .replace(/[̀-ͯ]/g, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'export'
  )
}

/** `fineme-comptes-foyer-2026-11-21.csv`. */
export function exportFileName(dataset: string, scope: string, day: string, extension: string): string {
  return `fineme-${slug(dataset)}-${slug(scope)}-${day}.${extension}`
}

export type AccountGroup = { kind: AccountKind; accounts: Account[]; total: number }

/** Comptes détenus par la vue, groupés par catégorie dans l'ordre de l'application (dettes en dernier). */
export function groupAccounts(accounts: Account[], memberId: string | null): AccountGroup[] {
  const held = accounts.filter((account) => viewShare(account.owners, memberId) > 0)
  return ACCOUNT_KIND_ORDER.flatMap((kind) => {
    const inKind = held
      .filter((account) => account.kind === kind)
      .sort((a, b) => b.owned_value - a.owned_value || a.name.localeCompare(b.name, 'fr'))
    if (inKind.length === 0) return []
    return [{ kind, accounts: inKind, total: inKind.reduce((sum, account) => sum + account.owned_value, 0) }]
  })
}

export type HistorySummary = {
  first: HistoryPoint
  last: HistoryPoint
  change: number
  /** Fraction ; null si le point de départ est nul. */
  changePct: number | null
  /** Dernier point connu de chaque mois, du plus ancien au plus récent. */
  monthly: HistoryPoint[]
}

/** Évolution sur la période : départ, arrivée, variation et un point par mois. `null` sans au moins deux points. */
export function summarizeHistory(points: HistoryPoint[], maxMonths = 12): HistorySummary | null {
  const sorted = [...points].sort((a, b) => a.date.localeCompare(b.date))
  if (sorted.length < 2) return null
  const first = sorted[0]
  const last = sorted[sorted.length - 1]
  const byMonth = new Map<string, HistoryPoint>()
  for (const point of sorted) byMonth.set(point.date.slice(0, 7), point)
  const change = last.net_worth - first.net_worth
  return {
    first,
    last,
    change,
    changePct: first.net_worth !== 0 ? change / Math.abs(first.net_worth) : null,
    monthly: [...byMonth.values()].slice(-maxMonths),
  }
}
