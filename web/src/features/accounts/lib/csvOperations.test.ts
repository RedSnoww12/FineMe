import { describe, expect, it } from 'vitest'
import {
  buildOperations,
  buildTransactions,
  columnsOf,
  guessOperationMapping,
  importReport,
  mappingFromColumns,
  operationMappingIssues,
  parseCsv,
  parseOperationKind,
  type Mapping,
  type OperationField,
} from '@/features/accounts/lib/csv'

const NONE: Mapping<OperationField> = {
  date: -1,
  kind: -1,
  name: -1,
  isin: -1,
  quantity: -1,
  unit_price: -1,
  amount: -1,
  debit: -1,
  credit: -1,
  fees: -1,
  note: -1,
}

describe('parseOperationKind', () => {
  it.each([
    ['Achat comptant', 'buy'],
    ['ACHAT', 'buy'],
    ['Souscription', 'buy'],
    ['Vente au marché', 'sell'],
    ['Cession', 'sell'],
    ['Coupon', 'dividend'],
    ['Dividende net', 'dividend'],
    ['Droits de garde', 'fee'],
    ['Frais sur achat', 'fee'],
    ['Taxe sur les transactions financières', 'fee'],
    ['Versement libre', 'deposit'],
    ['Dépôt d’espèces', 'deposit'],
    ['Retrait', 'withdrawal'],
    ['Rachat partiel', 'withdrawal'],
    ['Ordre exécuté : vente', 'sell'],
  ])('« %s » est un %s', (text, kind) => {
    expect(parseOperationKind(text)).toBe(kind)
  })

  it('renvoie null pour un libellé inconnu ou vide', () => {
    expect(parseOperationKind('Opération sur titres')).toBeNull()
    expect(parseOperationKind('  ')).toBeNull()
    // « achat » doit être un mot entier.
    expect(parseOperationKind('Rachats divers')).toBeNull()
  })
})

describe('buildOperations', () => {
  it('lit un historique d’ordres au format français (JJ/MM/AAAA, 1 234,56)', () => {
    const csv = parseCsv(
      [
        'Date d’exécution;Sens;Libellé valeur;Code ISIN;Quantité;Cours;Frais;Montant net',
        '03/02/2025;Achat;ETF Monde Fictif;fr0000000001;10;1 234,56;2,50;12 348,10',
        '15/06/2025;Vente;ETF Monde Fictif;FR0000000001;4;1 300,00;1,95;5 198,05',
        '02/07/2025;Coupon;ETF Monde Fictif;;;;;12,40',
        '05/01/2025;Versement;;;;;;1 000,00',
      ].join('\n'),
    )
    const mapping = guessOperationMapping(csv.headers)
    expect(mapping).toMatchObject({ date: 0, kind: 1, name: 2, isin: 3, quantity: 4, unit_price: 5, fees: 6, amount: 7 })
    expect(operationMappingIssues(mapping)).toEqual([])
    const { items, lines, errors } = buildOperations(csv.rows, mapping)
    expect(errors).toEqual([])
    expect(lines).toEqual([2, 3, 4, 5])
    expect(items).toEqual([
      {
        kind: 'buy',
        date: '2025-02-03',
        security: { name: 'ETF Monde Fictif', isin: 'FR0000000001' },
        quantity: 10,
        unit_price: 1234.56,
        fees: 2.5,
      },
      {
        kind: 'sell',
        date: '2025-06-15',
        security: { name: 'ETF Monde Fictif', isin: 'FR0000000001' },
        quantity: 4,
        unit_price: 1300,
        fees: 1.95,
      },
      { kind: 'dividend', date: '2025-07-02', security: { name: 'ETF Monde Fictif' }, amount: 12.4 },
      { kind: 'deposit', date: '2025-01-05', amount: 1000 },
    ])
  })

  it('lit débit et crédit séparés et déduit le cours du montant net et des frais', () => {
    const mapping = { ...NONE, date: 0, kind: 1, name: 2, quantity: 3, fees: 4, debit: 5, credit: 6, note: 7 }
    const rows = [
      // Achat : 1 005,00 débités dont 5,00 de frais → 4 titres à 250,00.
      ['10/03/2025', 'Achat', 'Action Fictive', '4', '5,00', '1 005,00', '', 'Ordre groupé'],
      // Vente : 495,00 crédités nets de 5,00 de frais → 2 titres à 250,00.
      ['12/03/2025', 'Vente', 'Action Fictive', '-2', '5,00', '', '495,00', ''],
      ['13/03/2025', 'Retrait', '', '', '', '200,00', '', ''],
      ['14/03/2025', 'Droits de garde', 'Action Fictive', '', '', '-3,00', '', ''],
    ]
    const { items, errors } = buildOperations(rows, mapping)
    expect(errors).toEqual([])
    expect(items[0]).toEqual({
      kind: 'buy',
      date: '2025-03-10',
      note: 'Ordre groupé',
      security: { name: 'Action Fictive' },
      quantity: 4,
      unit_price: 250,
      fees: 5,
    })
    expect(items[1]).toMatchObject({ kind: 'sell', quantity: 2, unit_price: 250, fees: 5 })
    expect(items[2]).toEqual({ kind: 'withdrawal', date: '2025-03-13', amount: 200 })
    expect(items[3]).toEqual({ kind: 'fee', date: '2025-03-14', security: { name: 'Action Fictive' }, amount: 3 })
  })

  it('rapporte chaque ligne illisible avec son numéro et sa raison', () => {
    const mapping = { ...NONE, date: 0, kind: 1, name: 2, quantity: 3, unit_price: 4, amount: 5 }
    const rows = [
      ['31/02/2025', 'Achat', 'A', '1', '1', ''],
      ['01/03/2025', 'Transfert', 'A', '1', '1', ''],
      ['01/03/2025', 'Achat', '', '1', '1', ''],
      ['01/03/2025', 'Achat', 'A', 'beaucoup', '1', ''],
      ['01/03/2025', 'Vente', 'A', '1', '', ''],
      ['01/03/2025', 'Dividende', 'A', '', '', ''],
      ['01/03/2025', 'Achat', 'A', '1', '10', ''],
    ]
    const { items, lines, errors } = buildOperations(rows, mapping)
    expect(errors).toEqual([
      { line: 2, reason: 'Date illisible : « 31/02/2025 »' },
      { line: 3, reason: 'Type d’opération non reconnu : « Transfert »' },
      { line: 4, reason: 'Titre manquant pour un achat ou une vente' },
      { line: 5, reason: 'Quantité illisible' },
      { line: 6, reason: 'Cours illisible' },
      { line: 7, reason: 'Montant illisible' },
    ])
    expect(items).toHaveLength(1)
    expect(lines).toEqual([8])
  })

  it('exige la date, le type et une quantité ou un montant', () => {
    expect(operationMappingIssues(NONE)).toHaveLength(3)
    expect(operationMappingIssues({ ...NONE, date: 0, kind: 1, credit: 2 })).toEqual([])
  })
})

describe('correspondance par nom de colonne', () => {
  it('mémorise les noms des colonnes utilisées puis les retrouve dans un autre ordre', () => {
    const headers = ['Date', 'Libellé', 'Débit', 'Crédit']
    const columns = columnsOf({ date: 0, label: 1, amount: -1, debit: 2, credit: 3, category: -1 }, headers)
    expect(columns).toEqual({ date: 'Date', label: 'Libellé', debit: 'Débit', credit: 'Crédit' })
    const fields = ['date', 'label', 'amount', 'debit', 'credit', 'category'] as const
    expect(mappingFromColumns(fields, columns, ['CRÉDIT', 'debit', 'libelle', 'date', 'Solde'])).toEqual({
      date: 3,
      label: 2,
      amount: -1,
      debit: 1,
      credit: 0,
      category: -1,
    })
    expect(mappingFromColumns(fields, columns, ['Date', 'Libellé', 'Montant'])).toBeNull()
  })
})

describe('importReport', () => {
  it('réunit les lignes illisibles et celles refusées par l’API, par numéro de ligne', () => {
    const built = buildTransactions(
      [
        ['01/01/2025', 'A', '1'],
        ['illisible', 'B', '2'],
        ['03/01/2025', 'C', '3'],
      ],
      { date: 0, label: 1, amount: 2, debit: -1, credit: -1, category: -1 },
    )
    const report = importReport(built, {
      inserted: 1,
      skipped: 0,
      errors: [{ index: 0, reason: 'Vente impossible' }],
    })
    expect(report).toEqual({
      added: 1,
      skipped: 0,
      errors: [
        { line: 2, reason: 'Vente impossible' },
        { line: 3, reason: 'Date illisible : « illisible »' },
      ],
    })
    expect(importReport(built, { inserted: 1, skipped: 1 }).skipped).toBe(1)
  })
})
