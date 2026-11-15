import { describe, expect, it } from 'vitest'
import {
  buildHoldings,
  buildTransactions,
  guessHoldingMapping,
  guessTransactionMapping,
  holdingMappingIssues,
  parseCsv,
  transactionMappingIssues,
} from '@/features/accounts/lib/csv'

describe('parseCsv', () => {
  it('détecte le point-virgule', () => {
    const csv = parseCsv('Date;Libellé;Montant\n01/02/2024;CB CARREFOUR;-45,90\n03/02/2024;VIR SALAIRE;2 500,00\n')
    expect(csv.delimiter).toBe(';')
    expect(csv.headers).toEqual(['Date', 'Libellé', 'Montant'])
    expect(csv.rows).toHaveLength(2)
  })
  it('détecte la virgule et respecte les guillemets', () => {
    const csv = parseCsv('date,label,amount\n2024-02-01,"Loyer, charges",-800.00\n')
    expect(csv.delimiter).toBe(',')
    expect(csv.rows[0]).toEqual(['2024-02-01', 'Loyer, charges', '-800.00'])
  })
  it('ignore le BOM et les lignes vides', () => {
    const csv = parseCsv('﻿Date;Montant\n\n01/01/2024;1\n\n')
    expect(csv.headers[0]).toBe('Date')
    expect(csv.rows).toHaveLength(1)
  })
  it('renvoie un résultat vide pour un fichier vide', () => {
    expect(parseCsv('').rows).toEqual([])
  })
})

describe('transactions', () => {
  it('devine les colonnes et construit les opérations', () => {
    const csv = parseCsv(
      'Date opération;Libellé;Montant;Catégorie\n01/02/2024;CB CARREFOUR;-45,90;Courses\n03/02/2024;VIR SALAIRE;2 500,00;\n',
    )
    const mapping = guessTransactionMapping(csv.headers)
    expect(mapping).toMatchObject({ date: 0, label: 1, amount: 2, category: 3 })
    expect(transactionMappingIssues(mapping)).toEqual([])
    const { items, errors } = buildTransactions(csv.rows, mapping)
    expect(errors).toEqual([])
    expect(items).toEqual([
      { date: '2024-02-01', label: 'CB CARREFOUR', amount: -45.9, category: 'Courses' },
      { date: '2024-02-03', label: 'VIR SALAIRE', amount: 2500 },
    ])
  })

  it('combine débit et crédit en montant signé', () => {
    const csv = parseCsv('Date;Libellé;Débit;Crédit\n01/02/2024;LOYER;800,00;\n05/02/2024;REMBOURSEMENT;;120,50\n')
    const mapping = guessTransactionMapping(csv.headers)
    expect(mapping).toMatchObject({ date: 0, label: 1, debit: 2, credit: 3, amount: -1 })
    const { items } = buildTransactions(csv.rows, mapping)
    expect(items.map((t) => t.amount)).toEqual([-800, 120.5])
  })

  it('signale les lignes illisibles avec leur numéro', () => {
    const csv = parseCsv('Date;Libellé;Montant\n32/01/2024;X;1\n01/01/2024;Y;abc\n01/01/2024;Z;3\n')
    const { items, errors } = buildTransactions(csv.rows, guessTransactionMapping(csv.headers))
    expect(items).toHaveLength(1)
    expect(errors.map((e) => e.line)).toEqual([2, 3])
  })

  it('exige date, libellé et montant', () => {
    expect(
      transactionMappingIssues({ date: -1, label: -1, amount: -1, debit: -1, credit: -1, category: -1 }),
    ).toHaveLength(3)
  })
})

describe('positions', () => {
  it('devine les colonnes et construit les positions', () => {
    const csv = parseCsv(
      'Nom;ISIN;Quantité;PRU;Cours\nAmundi MSCI World;FR0011869353;12,5;28,40;31,12\nLigne sans cours;;3;10;\n',
    )
    const mapping = guessHoldingMapping(csv.headers)
    expect(mapping).toEqual({ name: 0, isin: 1, quantity: 2, buy_price: 3, current_price: 4 })
    const { items, errors } = buildHoldings(csv.rows, mapping, 'EUR')
    expect(errors).toEqual([])
    expect(items[0]).toEqual({
      kind: 'security',
      name: 'Amundi MSCI World',
      isin: 'FR0011869353',
      quantity: 12.5,
      buy_price: 28.4,
      current_price: 31.12,
      currency: 'EUR',
    })
    // sans cours actuel, on reprend le prix d'achat
    expect(items[1]).toMatchObject({ isin: null, current_price: 10, buy_price: 10 })
  })
})

describe('cas limites de l’import', () => {
  it('nomme les colonnes sans en-tête', () => {
    expect(parseCsv('Date;;Montant\n01/01/2024;x;1\n').headers).toEqual(['Date', 'Colonne 2', 'Montant'])
  })
  it('détecte la tabulation', () => {
    expect(parseCsv('date\tlabel\tamount\n2024-01-01\tA\t1\n').delimiter).toBe('\t')
  })
  it('préfère le montant signé quand débit et crédit existent aussi', () => {
    const { items } = buildTransactions([['01/01/2024', 'A', '-12,00', '99', '']], {
      date: 0, label: 1, amount: 2, debit: 3, credit: 4, category: -1,
    })
    expect(items[0].amount).toBe(-12)
  })
  it('accepte un débit déjà négatif dans le fichier', () => {
    const { items } = buildTransactions([['01/01/2024', 'A', '-45,00', '']], {
      date: 0, label: 1, amount: -1, debit: 2, credit: 3, category: -1,
    })
    expect(items[0].amount).toBe(-45)
  })
  it('ignore un libellé vide et une ligne trop courte', () => {
    const { items, errors } = buildTransactions([['01/01/2024', '', '1'], ['02/01/2024']], {
      date: 0, label: 1, amount: 2, debit: -1, credit: -1, category: -1,
    })
    expect(items).toEqual([])
    expect(errors).toHaveLength(2)
  })
  it('rejette une position sans quantité ou sans aucun prix', () => {
    const mapping = { name: 0, isin: -1, quantity: 1, buy_price: 2, current_price: 3 }
    const { items, errors } = buildHoldings([['Fonds A', 'x', '1', '2'], ['Fonds B', '3', '', ''], ['', '1', '1', '1']], mapping, 'EUR')
    expect(items).toEqual([])
    expect(errors.map((e) => e.reason)).toEqual(['Quantité illisible', 'Prix illisible', 'Nom vide'])
  })
  it('écarte un ISIN mal formé sans rejeter la ligne', () => {
    const mapping = { name: 0, isin: 1, quantity: 2, buy_price: -1, current_price: 3 }
    const { items } = buildHoldings([['Fonds A', 'PASUNISIN', '2', '10']], mapping, 'USD')
    expect(items[0]).toMatchObject({ isin: null, buy_price: null, current_price: 10, currency: 'USD' })
  })
  it('signale un mapping de positions incomplet', () => {
    expect(holdingMappingIssues({ name: -1, isin: -1, quantity: -1, buy_price: -1, current_price: -1 })).toHaveLength(3)
    expect(holdingMappingIssues({ name: 0, isin: -1, quantity: 1, buy_price: 2, current_price: -1 })).toEqual([])
  })
})
