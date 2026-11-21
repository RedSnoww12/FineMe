import { describe, expect, it } from 'vitest'
import type { Account, AccountDetail, Holding, Transaction } from '@/shared/api/types'
import { accountsTable, csvDate, csvField, csvNumber, holdingsTable, toCsv, transactionsTable, viewShare } from './tabular'

const owners = [
  { member_id: 'm-a', name: 'Alex', share: 0.6 },
  { member_id: 'm-b', name: 'Bao', share: 0.4 },
]

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'a-1',
    name: 'PEA',
    institution: 'Banque Fictive',
    kind: 'investment',
    envelope: 'pea',
    currency: 'EUR',
    cash_balance: 0,
    annual_yield_pct: null,
    source: 'manual',
    external_id: null,
    connection_id: null,
    opened_at: null,
    owners,
    value: 1000,
    owned_value: 1000,
    is_liability: false,
    invested: 800,
    gain: 200,
    updated_at: '2026-03-04T10:00:00Z',
    ...overrides,
  }
}

const holding: Holding = {
  id: 'h-1',
  account_id: 'a-1',
  kind: 'security',
  name: 'ETF Monde',
  isin: 'FR0000000001',
  symbol: 'EWLD',
  security_type: 'etf',
  quantity: 10,
  buy_price: 80,
  current_price: 100,
  currency: 'EUR',
  expense_ratio: null,
  price_source: 'manual',
  price_updated_at: null,
  price_stale: false,
  value: 1000,
  invested: 800,
  gain: 200,
  gain_pct: 25,
}

describe('viewShare', () => {
  it('additionne les parts pour le foyer et ne garde que celle du membre sinon', () => {
    expect(viewShare(owners, null)).toBeCloseTo(1)
    expect(viewShare(owners, 'm-b')).toBe(0.4)
    expect(viewShare(owners, 'm-inconnu')).toBe(0)
  })

  it('garde une détention partielle du foyer (parts < 100 %)', () => {
    expect(viewShare([{ member_id: 'm-a', name: 'Alex', share: 0.5 }], null)).toBe(0.5)
  })
})

describe('format CSV français', () => {
  it('écrit les nombres avec une virgule décimale, sans séparateur de milliers', () => {
    expect(csvNumber(1234.56)).toBe('1234,56')
    expect(csvNumber(-0.5)).toBe('-0,5')
    expect(csvNumber(42)).toBe('42')
    expect(csvNumber(Number.NaN)).toBe('')
  })

  it('écrit les dates en JJ/MM/AAAA', () => {
    expect(csvDate('2026-03-04')).toBe('04/03/2026')
    expect(csvDate('2026-03-04T10:00:00Z')).toBe('04/03/2026')
  })

  it('entoure de guillemets les champs avec séparateur, guillemet ou retour à la ligne', () => {
    expect(csvField('Courses; marché')).toBe('"Courses; marché"')
    expect(csvField('Le « grand » "Bazar"')).toBe('"Le « grand » ""Bazar"""')
    expect(csvField('ligne 1\nligne 2')).toBe('"ligne 1\nligne 2"')
    expect(csvField('simple')).toBe('simple')
    expect(csvField(null)).toBe('')
    expect(csvField({ date: '2026-01-31' })).toBe('31/01/2026')
  })

  it('neutralise les textes qui seraient lus comme des formules, pas les nombres négatifs', () => {
    expect(csvField('=HYPERLINK("x")')).toBe('"\'=HYPERLINK(""x"")"')
    expect(csvField('-Retrait')).toBe("'-Retrait")
    expect(csvField('@cmd')).toBe("'@cmd")
    expect(csvField(-12.5)).toBe('-12,5')
  })

  it('commence par un BOM UTF-8 et sépare les lignes par CRLF', () => {
    const csv = toCsv({ name: 'T', columns: ['A', 'B'], rows: [[1.5, 'é']] })
    expect(csv.startsWith('﻿')).toBe(true)
    expect(csv.slice(1)).toBe('A;B\r\n1,5;é\r\n')
  })
})

describe('tableaux exportés', () => {
  it('liste les comptes avec la part de la vue et la valeur détenue renvoyée par l’API', () => {
    const table = accountsTable([account({ owned_value: 400 })], 'm-b')
    const row = table.rows[0]
    expect(row[table.columns.indexOf('Part de la vue (%)')]).toBe(40)
    expect(row[table.columns.indexOf('Valeur détenue (EUR)')]).toBe(400)
    expect(row[table.columns.indexOf('Détenteurs')]).toBe('Alex (60 %), Bao (40 %)')
    expect(row[table.columns.indexOf('Enveloppe')]).toBe('PEA')
    expect(row[table.columns.indexOf('Mis à jour le')]).toEqual({ date: '2026-03-04' })
  })

  it('applique la part du membre aux positions', () => {
    const detail: AccountDetail = { ...account(), holdings: [holding], property: null, loan: null }
    const table = holdingsTable([detail], 'm-b')
    const row = table.rows[0]
    expect(row[table.columns.indexOf('Valeur totale (EUR)')]).toBe(1000)
    expect(row[table.columns.indexOf('Valeur détenue (EUR)')]).toBe(400)
    expect(row[table.columns.indexOf('Investi détenu (EUR)')]).toBe(320)
    expect(row[table.columns.indexOf('Plus-value détenue (EUR)')]).toBe(80)
  })

  it('laisse vide l’investi inconnu au lieu d’inventer un zéro', () => {
    const detail: AccountDetail = {
      ...account(),
      holdings: [{ ...holding, buy_price: null, invested: null, gain: null, gain_pct: null }],
      property: null,
      loan: null,
    }
    const table = holdingsTable([detail], null)
    expect(table.rows[0][table.columns.indexOf('Investi détenu (EUR)')]).toBeNull()
  })

  it('préfère la catégorie manuelle et applique la part au montant', () => {
    const tx: Transaction = {
      id: 't-1',
      account_id: 'a-1',
      date: '2026-02-10',
      label: 'Supermarché',
      amount: -50,
      category: 'Divers',
      manual_category: 'Alimentation',
      internal_transfer: null,
    }
    const table = transactionsTable([{ account: account(), transactions: [tx] }], 'm-a')
    expect(table.rows[0]).toEqual(['PEA', { date: '2026-02-10' }, 'Supermarché', -50, 'EUR', 'Alimentation', 60, -30])
  })
})
