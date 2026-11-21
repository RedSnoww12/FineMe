import type { OperationInput, OperationKind } from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'

export const OPERATION_KIND_LABEL: Record<OperationKind, string> = {
  buy: 'Achat',
  sell: 'Vente',
  dividend: 'Dividende',
  fee: 'Frais',
  deposit: 'Versement',
  withdrawal: 'Retrait',
}

export const OPERATION_KINDS = Object.keys(OPERATION_KIND_LABEL) as OperationKind[]

/** Achat ou vente : porte une quantité et un cours, et fait varier la position. */
export function isTrade(kind: OperationKind): boolean {
  return kind === 'buy' || kind === 'sell'
}

/** Une position peut être rattachée (obligatoire pour un achat ou une vente). */
export function acceptsHolding(kind: OperationKind): boolean {
  return kind !== 'deposit' && kind !== 'withdrawal'
}

export type OperationForm = {
  kind: OperationKind
  date: string
  holding_id: string
  quantity: string
  unit_price: string
  fees: string
  amount: string
  note: string
}

export type OperationErrors = Partial<Record<'date' | 'holding_id' | 'quantity' | 'unit_price' | 'fees' | 'amount', string>>

export function emptyOperationForm(today: string): OperationForm {
  return { kind: 'buy', date: today, holding_id: '', quantity: '', unit_price: '', fees: '', amount: '', note: '' }
}

/**
 * Valide le formulaire et construit le corps de `POST /api/accounts/:id/operations`.
 * Seuls les champs utiles au type d'opération sont envoyés.
 */
export function buildOperationInput(
  form: OperationForm,
  today: string,
): { input: OperationInput; errors: null } | { input: null; errors: OperationErrors } {
  const errors: OperationErrors = {}
  if (!/^\d{4}-\d{2}-\d{2}$/.test(form.date)) errors.date = 'Saisissez la date de l’opération.'
  else if (form.date > today) errors.date = 'La date ne peut pas être dans le futur.'

  const input: OperationInput = { kind: form.kind, date: form.date }
  if (acceptsHolding(form.kind) && form.holding_id) input.holding_id = form.holding_id
  const note = form.note.trim()
  if (note) input.note = note

  if (isTrade(form.kind)) {
    if (!form.holding_id) errors.holding_id = 'Choisissez la position achetée ou vendue.'
    const quantity = parseFrNumber(form.quantity)
    const unitPrice = parseFrNumber(form.unit_price)
    const fees = form.fees.trim() ? parseFrNumber(form.fees) : 0
    if (quantity === null || quantity <= 0) errors.quantity = 'Saisissez une quantité positive, par exemple 12,5.'
    if (unitPrice === null || unitPrice < 0) errors.unit_price = 'Saisissez le cours unitaire.'
    if (fees === null || fees < 0) errors.fees = 'Saisissez les frais, ou laissez vide.'
    if (quantity !== null) input.quantity = quantity
    if (unitPrice !== null) input.unit_price = unitPrice
    if (fees !== null) input.fees = fees
  } else {
    const amount = parseFrNumber(form.amount)
    if (amount === null || amount <= 0) errors.amount = 'Saisissez un montant positif.'
    else input.amount = amount
  }

  return Object.keys(errors).length > 0 ? { input: null, errors } : { input, errors: null }
}

/**
 * Effet de l'opération sur les espèces du compte, signé (informatif : le journal ne modifie pas le solde).
 * Achat : −(montant + frais) ; vente : montant − frais ; dividende, versement : +montant ; frais, retrait : −montant.
 */
export function cashEffect(operation: { kind: OperationKind; amount: number; fees: number }): number {
  switch (operation.kind) {
    case 'buy':
      return -(operation.amount + operation.fees)
    case 'sell':
      return operation.amount - operation.fees
    case 'dividend':
    case 'deposit':
      return operation.amount
    case 'fee':
    case 'withdrawal':
      return -operation.amount
  }
}
