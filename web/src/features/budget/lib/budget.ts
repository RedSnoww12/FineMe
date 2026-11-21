import type { BudgetCategory, BudgetCategorySource, BudgetFlows, TransactionPatch } from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'

export const UNCATEGORIZED = 'Non catégorisé'

export function isMonth(value: string): boolean {
  return /^\d{4}-(0[1-9]|1[0-2])$/.test(value)
}

/** Décale un mois `YYYY-MM` de `delta` mois. */
export function shiftMonth(month: string, delta: number): string {
  const [year, index] = month.split('-').map(Number)
  const total = year * 12 + (index - 1) + delta
  return `${Math.floor(total / 12)}-${String((total % 12) + 1).padStart(2, '0')}`
}

const MONTH_FORMAT = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric', timeZone: 'UTC' })
const MONTH_SHORT = new Intl.DateTimeFormat('fr-FR', { month: 'short', timeZone: 'UTC' })

/** « janvier 2026 ». */
export function monthLabel(month: string): string {
  return MONTH_FORMAT.format(new Date(`${month}-01T00:00:00Z`))
}

/** « janv. » pour l'axe des graphiques. */
export function monthShort(month: string): string {
  return MONTH_SHORT.format(new Date(`${month}-01T00:00:00Z`))
}

export function categoryLabel(category: string | null): string {
  return category ?? UNCATEGORIZED
}

const SOURCE_LABELS: Record<BudgetCategorySource, string> = {
  manual: 'Choix manuel',
  rule: 'Règle',
  import: 'Banque',
  default: 'Automatique',
}

export function sourceLabel(source: BudgetCategorySource | null): string {
  return source ? SOURCE_LABELS[source] : 'Aucune règle'
}

export type TargetStatus = {
  /** Part de la cible consommée, en pourcentage (peut dépasser 100). */
  pct: number
  remaining: number
  over: boolean
}

/** Suivi d'une cible mensuelle ; `null` sans cible. Une cible à 0 est dépassée dès la première dépense. */
export function targetStatus(category: BudgetCategory): TargetStatus | null {
  if (category.target === null) return null
  const remaining = category.target - category.amount
  const pct = category.target > 0 ? (category.amount / category.target) * 100 : category.amount > 0 ? 100 : 0
  return { pct, remaining, over: remaining < 0 }
}

/** Mois sans aucun flux : rien à afficher. */
export function isEmptyMonth(flows: BudgetFlows, categories: BudgetCategory[]): boolean {
  return flows.income === 0 && flows.expenses === 0 && categories.every((category) => category.amount === 0)
}

/** Cible saisie : vide = suppression (`null`), sinon montant ≥ 0. */
export function parseTarget(raw: string): { value: number | null } | { error: string } {
  if (raw.trim() === '') return { value: null }
  const value = parseFrNumber(raw)
  if (value === null) return { error: 'Montant invalide.' }
  if (value < 0) return { error: 'Le montant ne peut pas être négatif.' }
  return { value }
}

/** Catégorie saisie : espaces réduits ; vide = retour à la catégorisation automatique (`null`). */
export function cleanCategory(raw: string): string | null {
  const name = raw.trim().replace(/\s+/g, ' ')
  return name === '' ? null : name
}

export type TransferChoice = 'auto' | 'yes' | 'no'

type Classified = {
  category: string | null
  category_source: BudgetCategorySource | null
  internal_transfer: boolean
  internal_transfer_manual: boolean
}

export function transferChoice(item: Pick<Classified, 'internal_transfer' | 'internal_transfer_manual'>): TransferChoice {
  if (!item.internal_transfer_manual) return 'auto'
  return item.internal_transfer ? 'yes' : 'no'
}

/**
 * Changements à envoyer pour une transaction : une catégorie différente de la catégorie effective, ou `null` pour
 * retirer un choix manuel ; le marquage de virement interne seulement s'il change.
 */
export function transactionPatch(item: Classified, input: { category: string; transfer: TransferChoice }): TransactionPatch {
  const patch: TransactionPatch = {}
  const category = cleanCategory(input.category)
  if (category === null ? item.category_source === 'manual' : category !== item.category) patch.category = category
  if (input.transfer !== transferChoice(item)) {
    patch.internal_transfer = input.transfer === 'auto' ? null : input.transfer === 'yes'
  }
  return patch
}
