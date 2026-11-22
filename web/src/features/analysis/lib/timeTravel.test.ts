import { describe, expect, it } from 'vitest'
import { buildHistoryAccount as line, buildHistoryPoint as point } from '@/test/fixtures'
import { compareSnapshots, quickDates, snapshotAt } from './timeTravel'

const january = point('2026-01-01', [
  line({ account_id: 'pea', name: 'PEA', kind: 'investment', value: 10_000 }),
  line({ account_id: 'livret', name: 'Livret A', kind: 'savings', value: 5_000 }),
  line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 8_000 }),
])
const june = point('2026-06-01', [
  line({ account_id: 'pea', name: 'PEA Camille', kind: 'investment', value: 13_000 }),
  line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 6_000 }),
  line({ account_id: 'btc', name: 'Bitcoin', kind: 'crypto', value: 1_000 }),
])

describe('snapshotAt', () => {
  const points = [june, january]

  it('prend le dernier instantané au plus tard à la date, quel que soit l’ordre reçu', () => {
    expect(snapshotAt(points, '2026-01-01')).toBe(january)
    expect(snapshotAt(points, '2026-05-31')).toBe(january)
    expect(snapshotAt(points, '2026-12-31')).toBe(june)
  })

  it('ne remonte pas avant le premier instantané', () => {
    expect(snapshotAt(points, '2025-12-31')).toBeNull()
    expect(snapshotAt([], '2026-01-01')).toBeNull()
  })
})

describe('compareSnapshots', () => {
  const comparison = compareSnapshots(january, june)

  it('donne l’écart des totaux', () => {
    expect(comparison.totals).toEqual([
      { key: 'gross_assets', label: 'Actifs bruts', from: 15_000, to: 14_000, delta: -1_000 },
      { key: 'liabilities', label: 'Dettes', from: 8_000, to: 6_000, delta: -2_000 },
      { key: 'net_worth', label: 'Patrimoine net', from: 7_000, to: 8_000, delta: 1_000 },
    ])
  })

  it('donne l’écart par catégorie d’actifs, sans les dettes', () => {
    expect(comparison.categories?.map((row) => [row.key, row.delta])).toEqual([
      ['savings', -5_000],
      ['investment', 3_000],
      ['crypto', 1_000],
    ])
  })

  it('donne l’écart par compte : ouvert ou clôturé entre-temps à 0, prêt en négatif, plus gros écart d’abord', () => {
    expect(comparison.accounts).toEqual([
      { key: 'livret', label: 'Livret A', from: 5_000, to: 0, delta: -5_000 },
      { key: 'pea', label: 'PEA Camille', from: 10_000, to: 13_000, delta: 3_000 },
      { key: 'pret', label: 'Prêt', from: -8_000, to: -6_000, delta: 2_000 },
      { key: 'btc', label: 'Bitcoin', from: 0, to: 1_000, delta: 1_000 },
    ])
  })

  it('sans détail par compte d’un côté, seuls les totaux sont comparés', () => {
    const old = point('2025-06-01', null, { gross_assets: 9_000, liabilities: 0, net_worth: 9_000 })
    const result = compareSnapshots(old, june)
    expect(result.categories).toBeNull()
    expect(result.accounts).toBeNull()
    expect(result.totals[2]?.delta).toBe(-1_000)
  })
})

describe('quickDates', () => {
  it('propose le 1er janvier, il y a un mois et il y a un an', () => {
    expect(quickDates('2026-11-22').map((q) => q.date)).toEqual(['2026-01-01', '2026-10-22', '2025-11-22'])
  })

  it('reste sur un jour qui existe et change d’année en janvier', () => {
    expect(quickDates('2026-03-31')[1]?.date).toBe('2026-02-28')
    expect(quickDates('2026-01-15')[1]?.date).toBe('2025-12-15')
    expect(quickDates('2024-02-29')[2]?.date).toBe('2023-02-28')
  })
})
