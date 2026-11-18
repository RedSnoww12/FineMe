import { describe, expect, it } from 'vitest'
import { fillMissingDays } from '@/shared/lib/history'
import { buildHistoryAccount as line, buildHistoryPoint as point } from '@/test/fixtures'
import { balanceRows, categoryRows, countUndetailedDays, presentCategories } from './evolution'

const day1 = point('2026-03-01', [
  line({ account_id: 'courant', kind: 'checking', value: 1000 }),
  line({ account_id: 'pea', kind: 'investment', value: 2000 }),
  line({ account_id: 'cto', kind: 'investment', value: 1000 }),
  line({ account_id: 'pret', kind: 'loan', value: 500 }),
])
const day2 = point('2026-03-02', [
  line({ account_id: 'courant', kind: 'checking', value: 2000 }),
  line({ account_id: 'pea', kind: 'investment', value: 3000 }),
  line({ account_id: 'pret', kind: 'loan', value: 0 }),
])

describe('évolution par catégorie', () => {
  it('additionne les comptes de chaque catégorie, sans les dettes', () => {
    const [first] = categoryRows(fillMissingDays([day1, day2]), 'eur')
    expect(first).toMatchObject({ date: '2026-03-01', estimated: false, checking: 1000, investment: 3000, real_estate: 0 })
  })

  it('exprime en pourcentage la part de chaque catégorie dans les actifs du jour', () => {
    const rows = categoryRows(fillMissingDays([day1, day2]), 'pct')
    expect(rows[0].checking).toBe(25)
    expect(rows[0].investment).toBe(75)
    expect(rows[1].checking).toBe(40)
  })

  it('rapporte les parts aux seules catégories affichées', () => {
    const rows = categoryRows(fillMissingDays([day1, day2]), 'pct', ['investment'])
    expect(rows[0].checking).toBe(100)
  })

  it('laisse un trou pour un jour sans détail par compte, et le compte', () => {
    const old = point('2026-02-28', null, { gross_assets: 3000, net_worth: 3000 })
    const series = fillMissingDays([old, day1])
    const rows = categoryRows(series, 'eur')
    expect(rows[0].checking).toBeNull()
    expect(rows[0].investment).toBeNull()
    expect(countUndetailedDays(series)).toBe(1)
  })

  it('ne garde que les catégories présentes, dans l’ordre d’empilement', () => {
    expect(presentCategories(categoryRows(fillMissingDays([day1, day2]), 'eur'))).toEqual(['checking', 'investment'])
  })

  it('n’affiche aucune part quand les actifs sont nuls', () => {
    const empty = point('2026-03-01', [line({ kind: 'loan', value: 200 })])
    expect(categoryRows(fillMissingDays([empty]), 'pct')[0].checking).toBe(0)
  })
})

describe('actifs, dettes et patrimoine net', () => {
  it('reprend les totaux en euros', () => {
    expect(balanceRows(fillMissingDays([day1]), 'eur')[0]).toMatchObject({
      gross_assets: 4000,
      liabilities: 500,
      net_worth: 3500,
    })
  })

  it('exprime en pourcentage la variation depuis le début de la période', () => {
    const rows = balanceRows(fillMissingDays([day1, day2]), 'pct')
    expect(rows[0]).toMatchObject({ gross_assets: 0, liabilities: 0, net_worth: 0 })
    expect(rows[1].gross_assets).toBe(25)
    expect(rows[1].liabilities).toBe(-100)
    expect(rows[1].net_worth).toBeCloseTo((5000 / 3500 - 1) * 100, 9)
  })

  it('n’invente pas de variation relative pour une série partie de zéro', () => {
    const noDebt = point('2026-03-01', [line({ value: 100 })])
    const withDebt = point('2026-03-02', [line({ value: 100 }), line({ account_id: 'pret', kind: 'loan', value: 50 })])
    expect(balanceRows(fillMissingDays([noDebt, withDebt]), 'pct')[1].liabilities).toBeNull()
  })
})
