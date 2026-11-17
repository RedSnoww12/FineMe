import { describe, expect, it } from 'vitest'
import { buildOverview } from '@/test/fixtures'
import { breakdownRows, OTHERS_KEY, toSlices } from './breakdown'

describe('répartition', () => {
  it('reprend l’allocation par catégorie avec ses couleurs', () => {
    const rows = breakdownRows(buildOverview(), 'category')
    expect(rows.map((r) => [r.key, r.value])).toEqual([
      ['investment', 33_000],
      ['savings', 12_000],
      ['real_estate', 135_000],
    ])
    expect(rows[0].color).toBe('var(--c1)')
  })

  it('complète les enveloppes par les actifs qui n’en ont pas', () => {
    const rows = breakdownRows(buildOverview(), 'envelope')
    expect(rows).toEqual([
      { key: 'pea', label: 'PEA', value: 33_000 },
      { key: 'sans-enveloppe', label: 'Sans enveloppe', value: 147_000 },
    ])
    const allInEnvelopes = buildOverview({ gross_assets: 33_000 })
    expect(breakdownRows(allInEnvelopes, 'envelope')).toHaveLength(1)
  })

  it('répartit par établissement et par membre', () => {
    expect(breakdownRows(buildOverview({ by_institution: [{ institution: '', value: 10 }] }), 'institution')[0].label).toBe(
      'Non renseigné',
    )
    const members = breakdownRows(buildOverview(), 'member')
    expect(members.map((m) => [m.label, m.value, m.color])).toEqual([
      ['Camille', 100_000, '#3F51B5'],
      ['Sacha', 80_000, '#B8862B'],
    ])
  })
})

describe('parts du graphique en anneau', () => {
  const row = (key: string, value: number) => ({ key, label: key, value })

  it('trie les parts positives et calcule leur pourcentage', () => {
    const slices = toSlices([row('a', 25), row('b', 75), row('c', 0), row('d', -10)])
    expect(slices.map((s) => [s.key, s.pct])).toEqual([
      ['b', 75],
      ['a', 25],
    ])
  })

  it('regroupe les plus petites parts dans « Autres »', () => {
    const slices = toSlices([row('a', 50), row('b', 20), row('c', 15), row('d', 10), row('e', 5)], 3)
    expect(slices.map((s) => [s.key, s.value])).toEqual([
      ['a', 50],
      ['b', 20],
      [OTHERS_KEY, 30],
    ])
    expect(slices.reduce((sum, s) => sum + s.pct, 0)).toBeCloseTo(100, 9)
  })

  it('ne renvoie rien sans actif', () => {
    expect(toSlices([row('a', 0)])).toEqual([])
  })
})
