import Papa from 'papaparse'
import type { HoldingInput, TransactionInput } from '@/shared/api/types'
import { parseDate, parseFrNumber } from '@/shared/lib/parse'

export type ParsedCsv = {
  headers: string[]
  rows: string[][]
  delimiter: string
}

/** Décode en UTF-8, avec repli sur Windows-1252 (exports bancaires français). */
export function decodeCsvBytes(buffer: ArrayBuffer): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(buffer)
  } catch {
    return new TextDecoder('windows-1252').decode(buffer)
  }
}

/** Détecte le séparateur et sépare la ligne d'en-tête des données. */
export function parseCsv(text: string): ParsedCsv {
  const result = Papa.parse<string[]>(text.replace(/^﻿/, ''), {
    delimiter: '',
    delimitersToGuess: [';', ',', '\t', '|'],
    skipEmptyLines: 'greedy',
    header: false,
  })
  const all = result.data.filter((row) => Array.isArray(row))
  if (all.length === 0) return { headers: [], rows: [], delimiter: result.meta.delimiter ?? ';' }
  const width = Math.max(...all.map((row) => row.length))
  const headers = Array.from({ length: width }, (_, i) => (all[0][i] ?? '').trim() || `Colonne ${i + 1}`)
  return { headers, rows: all.slice(1), delimiter: result.meta.delimiter ?? ';' }
}

export type ImportKind = 'transactions' | 'holdings'

export type TransactionField = 'date' | 'label' | 'amount' | 'debit' | 'credit' | 'category'
export type HoldingField = 'name' | 'isin' | 'quantity' | 'buy_price' | 'current_price'
/** Index de colonne par champ ; -1 = non utilisé. */
export type Mapping<F extends string> = Record<F, number>

const normalize = (s: string) =>
  s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()

const TRANSACTION_HINTS: Record<TransactionField, string[]> = {
  date: ['date operation', 'date de l operation', 'date comptable', 'date valeur', 'date'],
  label: ['libelle', 'label', 'description', 'intitule', 'detail', 'operation', 'motif'],
  amount: ['montant', 'amount', 'somme', 'valeur'],
  debit: ['debit'],
  credit: ['credit'],
  category: ['categorie', 'category', 'type'],
}

const HOLDING_HINTS: Record<HoldingField, string[]> = {
  name: ['nom', 'libelle', 'designation', 'valeur', 'titre', 'support', 'name', 'instrument'],
  isin: ['isin', 'code isin', 'code'],
  quantity: ['quantite', 'qte', 'nombre', 'parts', 'quantity', 'nb'],
  buy_price: ['pru', 'prix de revient', 'prix d achat', 'cours d achat', 'buy price', 'prix moyen', 'pamp'],
  current_price: ['cours', 'dernier cours', 'prix actuel', 'valeur liquidative', 'vl', 'price', 'prix'],
}

function guess<F extends string>(headers: string[], hints: Record<F, string[]>): Mapping<F> {
  const normalized = headers.map(normalize)
  const used = new Set<number>()
  const mapping = {} as Mapping<F>
  for (const field of Object.keys(hints) as F[]) {
    let found = -1
    for (const hint of hints[field]) {
      found = normalized.findIndex((h, i) => !used.has(i) && h === hint)
      if (found === -1) found = normalized.findIndex((h, i) => !used.has(i) && h.includes(hint))
      if (found !== -1) break
    }
    mapping[field] = found
    if (found !== -1) used.add(found)
  }
  return mapping
}

export const guessTransactionMapping = (headers: string[]) => guess(headers, TRANSACTION_HINTS)
export const guessHoldingMapping = (headers: string[]) => guess(headers, HOLDING_HINTS)

export type RowError = { line: number; reason: string }
export type BuildResult<T> = { items: T[]; errors: RowError[] }

const cell = (row: string[], index: number) => (index >= 0 ? (row[index] ?? '').trim() : '')

/** Liste les champs obligatoires manquants ; vide si le mapping est exploitable. */
export function transactionMappingIssues(mapping: Mapping<TransactionField>): string[] {
  const issues: string[] = []
  if (mapping.date < 0) issues.push('Choisissez la colonne de date.')
  if (mapping.label < 0) issues.push('Choisissez la colonne de libellé.')
  if (mapping.amount < 0 && mapping.debit < 0 && mapping.credit < 0)
    issues.push('Choisissez une colonne de montant, ou des colonnes débit et crédit.')
  return issues
}

export function holdingMappingIssues(mapping: Mapping<HoldingField>): string[] {
  const issues: string[] = []
  if (mapping.name < 0) issues.push('Choisissez la colonne du nom.')
  if (mapping.quantity < 0) issues.push('Choisissez la colonne de quantité.')
  if (mapping.current_price < 0 && mapping.buy_price < 0)
    issues.push('Choisissez au moins une colonne de prix (cours actuel ou prix d’achat).')
  return issues
}

/** Les numéros de ligne tiennent compte de l'en-tête (première ligne de données = ligne 2). */
export function buildTransactions(
  rows: string[][],
  mapping: Mapping<TransactionField>,
): BuildResult<TransactionInput> {
  const items: TransactionInput[] = []
  const errors: RowError[] = []
  rows.forEach((row, i) => {
    const line = i + 2
    const date = parseDate(cell(row, mapping.date))
    if (!date) {
      errors.push({ line, reason: `Date illisible : « ${cell(row, mapping.date)} »` })
      return
    }
    const label = cell(row, mapping.label)
    if (!label) {
      errors.push({ line, reason: 'Libellé vide' })
      return
    }
    let amount: number | null = null
    if (mapping.amount >= 0) amount = parseFrNumber(cell(row, mapping.amount))
    if (amount === null && (mapping.debit >= 0 || mapping.credit >= 0)) {
      const debit = parseFrNumber(cell(row, mapping.debit))
      const credit = parseFrNumber(cell(row, mapping.credit))
      if (debit !== null || credit !== null) amount = Math.abs(credit ?? 0) - Math.abs(debit ?? 0)
    }
    if (amount === null) {
      errors.push({ line, reason: 'Montant illisible' })
      return
    }
    const category = cell(row, mapping.category)
    items.push({ date, label, amount, ...(category ? { category } : {}) })
  })
  return { items, errors }
}

const ISIN = /^[A-Z]{2}[A-Z0-9]{9}\d$/

export function buildHoldings(
  rows: string[][],
  mapping: Mapping<HoldingField>,
  currency: string,
): BuildResult<HoldingInput> {
  const items: HoldingInput[] = []
  const errors: RowError[] = []
  rows.forEach((row, i) => {
    const line = i + 2
    const name = cell(row, mapping.name)
    if (!name) {
      errors.push({ line, reason: 'Nom vide' })
      return
    }
    const quantity = parseFrNumber(cell(row, mapping.quantity))
    if (quantity === null) {
      errors.push({ line, reason: 'Quantité illisible' })
      return
    }
    const buyPrice = parseFrNumber(cell(row, mapping.buy_price))
    const currentPrice = parseFrNumber(cell(row, mapping.current_price)) ?? buyPrice
    if (currentPrice === null) {
      errors.push({ line, reason: 'Prix illisible' })
      return
    }
    const isinRaw = cell(row, mapping.isin).toUpperCase().replace(/\s/g, '')
    items.push({
      kind: 'security',
      name,
      isin: ISIN.test(isinRaw) ? isinRaw : null,
      quantity,
      buy_price: buyPrice,
      current_price: currentPrice,
      currency,
    })
  })
  return { items, errors }
}
