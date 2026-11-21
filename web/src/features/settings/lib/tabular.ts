import type { Account, AccountDetail, AccountOwner, Transaction } from '@/shared/api/types'
import { ACCOUNT_KIND_LABEL, envelopeLabel, HOLDING_KIND_LABEL } from '@/shared/lib/labels'

/** Cellule d'un tableau exporté : texte, nombre, date `AAAA-MM-JJ` ou vide. */
export type Cell = string | number | { date: string } | null

export type Table = { name: string; columns: string[]; rows: Cell[][] }

/**
 * Part du compte retenue par la vue, comme l'API calcule `owned_value` :
 * somme des parts pour le foyer, part du membre sinon (0 s'il n'en détient aucune).
 */
export function viewShare(owners: AccountOwner[], memberId: string | null): number {
  const shares = memberId ? owners.filter((owner) => owner.member_id === memberId) : owners
  return shares.reduce((sum, owner) => sum + owner.share, 0)
}

function owned(value: number | null, share: number): number | null {
  return value === null ? null : round2(value * share)
}

function round2(value: number): number {
  return Math.round(value * 100) / 100
}

function sharePct(share: number): number {
  return round2(share * 100)
}

function ownersText(owners: AccountOwner[]): string {
  return owners.map((owner) => `${owner.name} (${sharePct(owner.share).toString().replace('.', ',')} %)`).join(', ')
}

/** La date seule d'un horodatage ISO, ou `null`. */
function dateOnly(value: string | null): Cell {
  if (!value) return null
  const match = /^\d{4}-\d{2}-\d{2}/.exec(value)
  return match ? { date: match[0] } : null
}

export function accountsTable(accounts: Account[], memberId: string | null): Table {
  return {
    name: 'Comptes',
    columns: [
      'Compte',
      'Établissement',
      'Catégorie',
      'Enveloppe',
      'Devise',
      'Détenteurs',
      'Part de la vue (%)',
      'Valeur totale (EUR)',
      'Valeur détenue (EUR)',
      'Dette',
      'Investi (EUR)',
      'Plus-value (EUR)',
      'Mis à jour le',
    ],
    rows: accounts.map((account) => [
      account.name,
      account.institution,
      ACCOUNT_KIND_LABEL[account.kind],
      account.envelope ? envelopeLabel(account.envelope) : null,
      account.currency,
      ownersText(account.owners),
      sharePct(viewShare(account.owners, memberId)),
      account.value,
      account.owned_value,
      account.is_liability ? 'Oui' : 'Non',
      account.invested,
      account.gain,
      dateOnly(account.updated_at),
    ]),
  }
}

export function holdingsTable(details: AccountDetail[], memberId: string | null): Table {
  return {
    name: 'Positions',
    columns: [
      'Compte',
      'Position',
      'Type',
      'ISIN',
      'Symbole',
      'Quantité',
      'PRU',
      'Cours',
      'Devise',
      'Valeur totale (EUR)',
      'Part de la vue (%)',
      'Valeur détenue (EUR)',
      'Investi détenu (EUR)',
      'Plus-value détenue (EUR)',
      'Plus-value (%)',
    ],
    rows: details.flatMap((detail) => {
      const share = viewShare(detail.owners, memberId)
      return detail.holdings.map((holding) => [
        detail.name,
        holding.name,
        HOLDING_KIND_LABEL[holding.kind],
        holding.isin,
        holding.symbol,
        holding.quantity,
        holding.buy_price,
        holding.current_price,
        holding.currency,
        holding.value,
        sharePct(share),
        owned(holding.value, share),
        owned(holding.invested, share),
        owned(holding.gain, share),
        holding.gain_pct,
      ])
    }),
  }
}

export type AccountTransactions = { account: Account; transactions: Transaction[] }

export function transactionsTable(groups: AccountTransactions[], memberId: string | null): Table {
  return {
    name: 'Transactions',
    columns: ['Compte', 'Date', 'Libellé', 'Montant', 'Devise', 'Catégorie', 'Part de la vue (%)', 'Montant détenu'],
    rows: groups.flatMap(({ account, transactions }) => {
      const share = viewShare(account.owners, memberId)
      return transactions.map((tx) => [
        account.name,
        { date: tx.date },
        tx.label,
        tx.amount,
        account.currency,
        tx.manual_category ?? tx.category,
        sharePct(share),
        owned(tx.amount, share),
      ])
    }),
  }
}

// --- CSV « à la française » : point-virgule, virgule décimale, dates JJ/MM/AAAA, BOM pour Excel ---

export const CSV_SEPARATOR = ';'
const BOM = '﻿'

/** `1234.5` → `1234,5` : sans séparateur de milliers, pour rester lisible par un tableur. */
export function csvNumber(value: number): string {
  if (!Number.isFinite(value)) return ''
  return String(value).replace('.', ',')
}

/** `2025-03-01` → `01/03/2025`. */
export function csvDate(iso: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso)
  return match ? `${match[3]}/${match[2]}/${match[1]}` : iso
}

/** Un texte qui commence par `= + - @` serait lu comme une formule par le tableur : on le neutralise. */
function neutralizeFormula(text: string): string {
  return /^[=+\-@\t\r]/.test(text) ? `'${text}` : text
}

export function csvField(cell: Cell): string {
  if (cell === null) return ''
  if (typeof cell === 'number') return csvNumber(cell)
  const text = typeof cell === 'string' ? neutralizeFormula(cell) : csvDate(cell.date)
  return /[";\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text
}

/** Fichier CSV complet, BOM UTF-8 en tête pour qu'Excel lise les accents. */
export function toCsv(table: Table): string {
  const lines = [table.columns, ...table.rows].map((row) => row.map(csvField).join(CSV_SEPARATOR))
  return BOM + lines.join('\r\n') + '\r\n'
}
