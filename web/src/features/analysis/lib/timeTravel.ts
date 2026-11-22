// Patrimoine à une date passée et comparaison de deux dates, à partir des instantanés quotidiens.
import type { AllocationCategory, HistoryPoint } from '@/shared/api/types'
import { ACCOUNT_KIND_PLURAL } from '@/shared/lib/labels'
import { ASSET_CATEGORIES } from './evolution'

/** Instantané en vigueur à `date` : le dernier relevé ce jour-là ou avant ; null si l'historique commence après. */
export function snapshotAt(points: readonly HistoryPoint[], date: string): HistoryPoint | null {
  let found: HistoryPoint | null = null
  for (const point of points) {
    if (point.date <= date && (found === null || point.date > found.date)) found = point
  }
  return found
}

export type DeltaRow = { key: string; label: string; from: number; to: number; delta: number }

function deltaRow(key: string, label: string, from: number, to: number): DeltaRow {
  return { key, label, from, to, delta: to - from }
}

export type Comparison = {
  totals: DeltaRow[]
  /** null : l'un des deux instantanés n'a pas de détail par compte. */
  categories: DeltaRow[] | null
  accounts: DeltaRow[] | null
}

function byCategory(point: HistoryPoint): Record<AllocationCategory, number> {
  const values = { checking: 0, savings: 0, investment: 0, crypto: 0, real_estate: 0, other: 0 }
  for (const account of point.accounts ?? []) {
    if (account.kind !== 'loan') values[account.kind] += account.value
  }
  return values
}

/**
 * Écarts entre deux instantanés : totaux, catégories d'actifs et comptes (un prêt compte en négatif). Un compte
 * présent à une seule date vaut 0 à l'autre ; il garde le nom de son dernier relevé.
 */
export function compareSnapshots(from: HistoryPoint, to: HistoryPoint): Comparison {
  const totals = [
    deltaRow('gross_assets', 'Actifs bruts', from.gross_assets, to.gross_assets),
    deltaRow('liabilities', 'Dettes', from.liabilities, to.liabilities),
    deltaRow('net_worth', 'Patrimoine net', from.net_worth, to.net_worth),
  ]
  if (from.accounts === null || to.accounts === null) return { totals, categories: null, accounts: null }

  const before = byCategory(from)
  const after = byCategory(to)
  const categories = ASSET_CATEGORIES.filter((c) => before[c] !== 0 || after[c] !== 0).map((c) =>
    deltaRow(c, ACCOUNT_KIND_PLURAL[c], before[c], after[c]),
  )

  const rows = new Map<string, { label: string; from: number; to: number }>()
  const signed = (kind: string, value: number) => (kind === 'loan' ? -value : value)
  for (const account of from.accounts) {
    rows.set(account.account_id, { label: account.name, from: signed(account.kind, account.value), to: 0 })
  }
  for (const account of to.accounts) {
    const row = rows.get(account.account_id) ?? { label: account.name, from: 0, to: 0 }
    rows.set(account.account_id, { ...row, label: account.name, to: signed(account.kind, account.value) })
  }
  const accounts = [...rows.entries()]
    .map(([key, row]) => deltaRow(key, row.label, row.from, row.to))
    .sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta))

  return { totals, categories, accounts }
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
