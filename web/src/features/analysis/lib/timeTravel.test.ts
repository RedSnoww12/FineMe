import { describe, expect, it } from 'vitest'
import type { HistoryComparison } from '@/shared/api/types'
import { isIsoDate, labelComparison, quickDates } from './timeTravel'

const comparison: HistoryComparison = {
  first_date: '2025-12-31',
  a: { requested: '2026-01-15', date: '2025-12-31' },
  b: { requested: '2026-06-01', date: '2026-06-01' },
  totals: [
    { key: 'gross_assets', from: 10_000, to: 15_500, delta: 5_500 },
    { key: 'liabilities', from: 4_000, to: 3_000, delta: -1_000 },
    { key: 'net_worth', from: 6_000, to: 12_500, delta: 6_500 },
  ],
  categories: [{ key: 'savings', from: 0, to: 3_000, delta: 3_000 }],
  accounts: [
    {
      key: 'pret',
      name: 'Prêt',
      kind: 'loan',
      from: -4_000,
      to: -3_000,
      delta: 1_000,
    },
  ],
  members: [{ key: 'm1', name: 'Camille', from: 6_000, to: 12_500, delta: 6_500 }],
}

describe('libellés de la comparaison', () => {
  it('nomme les totaux, les catégories, les comptes et les membres', () => {
    const labeled = labelComparison(comparison)
    expect(labeled?.totals.map((row) => row.label)).toEqual(['Actifs bruts', 'Dettes', 'Patrimoine net'])
    expect(labeled?.categories?.[0]).toEqual({
      key: 'savings',
      label: 'Épargne',
      from: 0,
      to: 3_000,
      delta: 3_000,
    })
    expect(labeled?.accounts?.[0]?.label).toBe('Prêt')
    expect(labeled?.members?.[0]?.label).toBe('Camille')
  })

  it('garde une clé inconnue telle quelle et ne nomme rien sans instantané', () => {
    const unknown = {
      ...comparison,
      categories: [{ key: 'futur', from: 1, to: 2, delta: 1 }],
    }
    expect(labelComparison(unknown)?.categories?.[0]?.label).toBe('futur')
    const empty = {
      ...comparison,
      a: null,
      totals: null,
      categories: null,
      accounts: null,
      members: null,
    }
    expect(labelComparison(empty)).toBeNull()
  })

  it('laisse le détail absent quand l’instantané n’en a pas', () => {
    const labeled = labelComparison({
      ...comparison,
      categories: null,
      accounts: null,
      members: null,
    })
    expect(labeled?.categories).toBeNull()
    expect(labeled?.accounts).toBeNull()
  })
})

describe('isIsoDate', () => {
  it('accepte une date du calendrier et refuse un champ vide ou une date impossible', () => {
    expect(isIsoDate('2026-02-28')).toBe(true)
    expect(isIsoDate('')).toBe(false)
    expect(isIsoDate('2026-02-30')).toBe(false)
    expect(isIsoDate('28/02/2026')).toBe(false)
  })
})

describe('quickDates', () => {
  it('propose le 1er janvier, il y a un mois et il y a un an', () => {
    expect(quickDates('2026-11-22').map((q) => q.date)).toEqual(['2026-01-01', '2026-10-22', '2025-11-22'])
  })

  it('ramène un jour inexistant au dernier jour du mois', () => {
    expect(quickDates('2026-03-31')[1]?.date).toBe('2026-02-28')
    expect(quickDates('2026-01-15')[1]?.date).toBe('2025-12-15')
    expect(quickDates('2024-02-29')[2]?.date).toBe('2023-02-28')
  })
})
