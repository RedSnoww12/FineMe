import Papa from 'papaparse'
import type { HoldingInput, ImportKind, OperationImportInput, OperationKind, TransactionInput } from '@/shared/api/types'
import { parseDate, parseFrNumber } from '@/shared/lib/parse'

export type { ImportKind }

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

export type TransactionField = 'date' | 'label' | 'amount' | 'debit' | 'credit' | 'category'
export type HoldingField = 'name' | 'isin' | 'quantity' | 'buy_price' | 'current_price'
export type OperationField =
  | 'date'
  | 'kind'
  | 'name'
  | 'isin'
  | 'quantity'
  | 'unit_price'
  | 'amount'
  | 'debit'
  | 'credit'
  | 'fees'
  | 'note'
/** Index de colonne par champ ; -1 = non utilisé. */
export type Mapping<F extends string> = Record<F, number>

/** Normalise un en-tête ou un libellé : minuscules, sans accents ni ponctuation. */
export const normalize = (s: string) =>
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

const OPERATION_HINTS: Record<OperationField, string[]> = {
  date: ['date d execution', 'date operation', 'date d operation', 'date', 'jour'],
  kind: ['sens', 'type d operation', 'type operation', 'nature', 'type', 'operation'],
  name: ['libelle valeur', 'valeur', 'titre', 'instrument', 'support', 'libelle', 'designation', 'nom', 'produit'],
  isin: ['code isin', 'isin'],
  quantity: ['quantite', 'qte', 'nombre de parts', 'nombre', 'parts'],
  unit_price: ['prix unitaire', 'cours d execution', 'cours', 'prix'],
  fees: ['frais', 'commission', 'courtage'],
  amount: ['montant net', 'montant', 'net'],
  debit: ['debit'],
  credit: ['credit'],
  note: ['commentaire', 'note', 'remarque'],
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
export const guessOperationMapping = (headers: string[]) => guess(headers, OPERATION_HINTS)

/** Champs de chaque type d'import, dans l'ordre d'affichage. */
export const IMPORT_FIELDS = {
  transactions: Object.keys(TRANSACTION_HINTS) as TransactionField[],
  holdings: Object.keys(HOLDING_HINTS) as HoldingField[],
  operations: Object.keys(OPERATION_HINTS) as OperationField[],
} as const

/** Correspondance par nom de colonne (ce qui est mémorisé) : survit à un changement d'ordre des colonnes. */
export function columnsOf<F extends string>(mapping: Mapping<F>, headers: string[]): Record<string, string> {
  const columns: Record<string, string> = {}
  for (const [field, index] of Object.entries(mapping) as [F, number][]) {
    if (index >= 0 && headers[index]) columns[field] = headers[index]
  }
  return columns
}

/**
 * Applique une correspondance par nom de colonne aux en-têtes d'un fichier (casse et accents ignorés).
 * `null` si une colonne attendue manque : la correspondance ne convient pas à ce fichier.
 */
export function mappingFromColumns<F extends string>(
  fields: readonly F[],
  columns: Record<string, string>,
  headers: string[],
): Mapping<F> | null {
  const normalized = headers.map(normalize)
  const mapping = {} as Mapping<F>
  for (const field of fields) {
    const header = columns[field]
    if (header === undefined) {
      mapping[field] = -1
      continue
    }
    const index = normalized.indexOf(normalize(header))
    if (index === -1) return null
    mapping[field] = index
  }
  return mapping
}

export type RowError = { line: number; reason: string }
/** `lines[i]` : numéro de ligne du fichier d'où vient `items[i]`. */
export type BuildResult<T> = { items: T[]; lines: number[]; errors: RowError[] }

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

export function operationMappingIssues(mapping: Mapping<OperationField>): string[] {
  const issues: string[] = []
  if (mapping.date < 0) issues.push('Choisissez la colonne de date.')
  if (mapping.kind < 0) issues.push('Choisissez la colonne du type d’opération (achat, vente…).')
  if (mapping.quantity < 0 && mapping.amount < 0 && mapping.debit < 0 && mapping.credit < 0)
    issues.push('Choisissez au moins une colonne de quantité ou de montant.')
  return issues
}

/** Les numéros de ligne tiennent compte de l'en-tête (première ligne de données = ligne 2). */
export function buildTransactions(
  rows: string[][],
  mapping: Mapping<TransactionField>,
): BuildResult<TransactionInput> {
  const items: TransactionInput[] = []
  const lines: number[] = []
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
    lines.push(line)
  })
  return { items, lines, errors }
}

const ISIN = /^[A-Z]{2}[A-Z0-9]{9}\d$/

export function buildHoldings(
  rows: string[][],
  mapping: Mapping<HoldingField>,
  currency: string,
): BuildResult<HoldingInput> {
  const items: HoldingInput[] = []
  const lines: number[] = []
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
    lines.push(line)
  })
  return { items, lines, errors }
}

/** Mots reconnus pour chaque type d'opération (normalisés). L'ordre départage « Frais sur achat » (= frais). */
const OPERATION_KIND_WORDS: [OperationKind, string[]][] = [
  ['fee', ['frais', 'commission', 'droits de garde', 'taxe', 'fee', 'fees']],
  ['dividend', ['dividende', 'coupon', 'distribution', 'dividend']],
  ['withdrawal', ['retrait', 'rachat partiel', 'rachat', 'virement emis', 'withdrawal']],
  ['deposit', ['versement', 'depot', 'apport', 'virement recu', 'alimentation', 'deposit']],
  ['sell', ['vente', 'cession', 'sell']],
  ['buy', ['achat', 'souscription', 'acquisition', 'buy']],
]

/**
 * Lit le type d'une opération dans un libellé de courtier (« Achat comptant », « VENTE », « Coupon »…).
 * Un libellé qui commence par un mot connu l'emporte (« Frais sur achat » = frais), puis un libellé qui en contient un.
 */
export function parseOperationKind(raw: string): OperationKind | null {
  const text = normalize(raw)
  if (!text) return null
  const startsWith = (word: string) => text === word || text.startsWith(`${word} `)
  const contains = (word: string) => ` ${text} `.includes(` ${word} `)
  for (const test of [startsWith, contains]) {
    const found = OPERATION_KIND_WORDS.find(([, words]) => words.some(test))
    if (found) return found[0]
  }
  return null
}

export type ImportReport = { added: number; skipped: number; errors: RowError[] }

/**
 * Bilan d'un import : lignes ajoutées, ignorées (déjà présentes) et en erreur, avec leur numéro de ligne dans le
 * fichier. Réunit les lignes illisibles côté navigateur et celles refusées par l'API (`index` = rang de l'élément
 * envoyé, retrouvé dans `lines`).
 */
export function importReport(
  built: { lines: number[]; errors: RowError[] },
  result: { inserted: number; skipped: number; errors?: { index: number; reason: string }[] },
): ImportReport {
  const refused = (result.errors ?? []).map((error) => ({ line: built.lines[error.index] ?? 0, reason: error.reason }))
  const errors = [...built.errors, ...refused].sort((a, b) => a.line - b.line)
  return { added: result.inserted, skipped: result.skipped, errors }
}

const TRADES: OperationKind[] = ['buy', 'sell']

/** Montant de la ligne en valeur absolue : colonne « montant », sinon débit ou crédit. */
function rowAmount(row: string[], mapping: Mapping<OperationField>): number | null {
  const amount = parseFrNumber(cell(row, mapping.amount))
  if (amount !== null) return Math.abs(amount)
  const debit = parseFrNumber(cell(row, mapping.debit))
  const credit = parseFrNumber(cell(row, mapping.credit))
  if (debit === null && credit === null) return null
  return Math.abs(debit ?? 0) + Math.abs(credit ?? 0)
}

const round = (value: number, decimals = 6) => Math.round(value * 10 ** decimals) / 10 ** decimals

/**
 * Construit les lignes du journal d'opérations. Achat ou vente : quantité et cours ; sans cours, il se déduit du
 * montant net (achat : net = brut + frais ; vente : net = brut − frais). Autres types : montant > 0.
 */
export function buildOperations(
  rows: string[][],
  mapping: Mapping<OperationField>,
): BuildResult<OperationImportInput> {
  const items: OperationImportInput[] = []
  const lines: number[] = []
  const errors: RowError[] = []
  rows.forEach((row, i) => {
    const line = i + 2
    const date = parseDate(cell(row, mapping.date))
    if (!date) {
      errors.push({ line, reason: `Date illisible : « ${cell(row, mapping.date)} »` })
      return
    }
    const kindText = cell(row, mapping.kind)
    const kind = parseOperationKind(kindText)
    if (!kind) {
      errors.push({ line, reason: `Type d’opération non reconnu : « ${kindText} »` })
      return
    }
    const name = cell(row, mapping.name)
    const isinRaw = cell(row, mapping.isin).toUpperCase().replace(/\s/g, '')
    const isin = ISIN.test(isinRaw) ? isinRaw : null
    const security = name || isin ? { name: name || (isin ?? ''), ...(isin ? { isin } : {}) } : null
    const note = cell(row, mapping.note)
    const amount = rowAmount(row, mapping)
    const base: OperationImportInput = { kind, date, ...(note ? { note } : {}) }

    if (TRADES.includes(kind)) {
      if (!security) {
        errors.push({ line, reason: 'Titre manquant pour un achat ou une vente' })
        return
      }
      const quantity = parseFrNumber(cell(row, mapping.quantity))
      if (quantity === null || quantity === 0) {
        errors.push({ line, reason: 'Quantité illisible' })
        return
      }
      const fees = Math.abs(parseFrNumber(cell(row, mapping.fees)) ?? 0)
      let unitPrice = parseFrNumber(cell(row, mapping.unit_price))
      if (unitPrice === null && amount !== null) {
        const gross = kind === 'buy' ? amount - fees : amount + fees
        unitPrice = round(gross / Math.abs(quantity))
      }
      if (unitPrice === null || unitPrice < 0) {
        errors.push({ line, reason: 'Cours illisible' })
        return
      }
      items.push({ ...base, security, quantity: Math.abs(quantity), unit_price: unitPrice, fees })
    } else {
      if (amount === null || amount === 0) {
        errors.push({ line, reason: 'Montant illisible' })
        return
      }
      const attached = security && (kind === 'dividend' || kind === 'fee') ? { security } : {}
      items.push({ ...base, ...attached, amount })
    }
    lines.push(line)
  })
  return { items, lines, errors }
}
