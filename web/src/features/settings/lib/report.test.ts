import { describe, expect, it } from 'vitest'
import type { Account, HistoryPoint } from '@/shared/api/types'
import { exportFileName, groupAccounts, slug, summarizeHistory } from './report'

function point(date: string, net: number): HistoryPoint {
  return { date, gross_assets: net + 100, liabilities: 100, net_worth: net, accounts: null }
}

function account(id: string, kind: Account['kind'], owned: number, memberIds = ['m-a']): Account {
  return {
    id,
    name: `Compte ${id}`,
    institution: null,
    kind,
    envelope: null,
    currency: 'EUR',
    cash_balance: 0,
    annual_yield_pct: null,
    management_fee_pct: null,
    source: 'manual',
    external_id: null,
    connection_id: null,
    opened_at: null,
    owners: memberIds.map((member_id) => ({ member_id, name: member_id, share: 1 / memberIds.length })),
    value: owned,
    owned_value: owned,
    is_liability: kind === 'loan',
    invested: null,
    gain: null,
    updated_at: '2026-01-01T00:00:00Z',
  }
}

describe('nom des fichiers', () => {
  it('retire accents, espaces et ponctuation', () => {
    expect(slug('Zoé Lefèvre-Ünal')).toBe('zoe-lefevre-unal')
    expect(slug('  ')).toBe('export')
    expect(exportFileName('Transactions', 'Foyer', '2026-11-21', 'csv')).toBe('fineme-transactions-foyer-2026-11-21.csv')
  })
})

describe('groupAccounts', () => {
  it('groupe par catégorie dans l’ordre de l’application, dettes en dernier, plus gros comptes d’abord', () => {
    const groups = groupAccounts(
      [account('p', 'loan', 50), account('s1', 'savings', 10), account('c', 'checking', 5), account('s2', 'savings', 30)],
      null,
    )
    expect(groups.map((g) => g.kind)).toEqual(['checking', 'savings', 'loan'])
    expect(groups[1].accounts.map((a) => a.id)).toEqual(['s2', 's1'])
    expect(groups[1].total).toBe(40)
  })

  it('écarte les comptes où le membre n’a aucune part', () => {
    const groups = groupAccounts([account('a', 'checking', 5, ['m-a']), account('b', 'checking', 5, ['m-b'])], 'm-b')
    expect(groups[0].accounts.map((a) => a.id)).toEqual(['b'])
  })
})

describe('summarizeHistory', () => {
  it('exige au moins deux points', () => {
    expect(summarizeHistory([])).toBeNull()
    expect(summarizeHistory([point('2026-01-01', 10)])).toBeNull()
  })

  it('calcule la variation et garde le dernier point de chaque mois', () => {
    const summary = summarizeHistory([
      point('2026-02-15', 1500),
      point('2026-01-01', 1000),
      point('2026-01-31', 1200),
      point('2026-02-28', 1100),
    ])
    expect(summary?.change).toBe(100)
    expect(summary?.changePct).toBeCloseTo(0.1)
    expect(summary?.monthly.map((p) => p.date)).toEqual(['2026-01-31', '2026-02-28'])
  })

  it('ne calcule pas de pourcentage depuis un patrimoine nul', () => {
    expect(summarizeHistory([point('2026-01-01', 0), point('2026-02-01', 500)])?.changePct).toBeNull()
  })

  it('limite le nombre de mois', () => {
    const points = Array.from({ length: 15 }, (_, i) => point(`20${25 + Math.floor(i / 12)}-${String((i % 12) + 1).padStart(2, '0')}-10`, i))
    expect(summarizeHistory(points, 12)?.monthly).toHaveLength(12)
  })
})
