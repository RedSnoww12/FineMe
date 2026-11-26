// Patrimoine à une date et comparaison de deux dates : les écarts sont calculés par l'API
// (`GET /api/history/compare`) ; ce module ne fait que les présenter (libellés, raccourcis de date).
import type { HistoryComparison } from '@/shared/api/types'
import { ACCOUNT_KIND_PLURAL } from '@/shared/lib/labels'

export type DeltaRow = {
  key: string
  label: string
  from: number
  to: number
  delta: number
}

export type LabeledComparison = {
  totals: DeltaRow[]
  /** null : l'un des deux instantanés n'a pas de détail par compte. */
  categories: DeltaRow[] | null
  accounts: DeltaRow[] | null
  /** Vue foyer seulement. */
  members: DeltaRow[] | null
}

const TOTAL_LABELS: Record<string, string> = {
  gross_assets: 'Actifs bruts',
  liabilities: 'Dettes',
  net_worth: 'Patrimoine net',
}

const CATEGORY_LABELS: Record<string, string> = ACCOUNT_KIND_PLURAL

/** Libellés des lignes d'une comparaison ; null si l'une des deux dates n'a pas d'instantané. */
export function labelComparison(comparison: HistoryComparison): LabeledComparison | null {
  if (comparison.totals === null) return null
  const withLabel =
    (labels: Record<string, string>) =>
    (row: { key: string; from: number; to: number; delta: number }): DeltaRow => ({
      ...row,
      label: labels[row.key] ?? row.key,
    })
  const named = (row: { key: string; name: string; from: number; to: number; delta: number }): DeltaRow => ({
    key: row.key,
    label: row.name,
    from: row.from,
    to: row.to,
    delta: row.delta,
  })
  return {
    totals: comparison.totals.map(withLabel(TOTAL_LABELS)),
    categories: comparison.categories?.map(withLabel(CATEGORY_LABELS)) ?? null,
    accounts: comparison.accounts?.map(named) ?? null,
    members: comparison.members?.map(named) ?? null,
  }
}

/** Vrai pour une date AAAA-MM-JJ qui existe au calendrier (un champ date vidé donne une chaîne vide). */
export function isIsoDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
  const parsed = new Date(`${value}T00:00:00Z`)
  return !Number.isNaN(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value
}

/** Raccourcis de date : 1er janvier de l'année, il y a un mois, il y a un an. */
export function quickDates(today: string): { label: string; date: string }[] {
  const [year, month, day] = today.split('-').map(Number)
  const iso = (y: number, m: number, d: number) => {
    // Le 31 d'un mois plus court devient le dernier jour de ce mois.
    const last = new Date(Date.UTC(y, m, 0)).getUTCDate()
    return `${y}-${String(m).padStart(2, '0')}-${String(Math.min(d, last)).padStart(2, '0')}`
  }
  const previousMonth = month === 1 ? iso(year - 1, 12, day) : iso(year, month - 1, day)
  return [
    { label: '1er janvier', date: `${year}-01-01` },
    { label: 'Il y a un mois', date: previousMonth },
    { label: 'Il y a un an', date: iso(year - 1, month, day) },
  ]
}
