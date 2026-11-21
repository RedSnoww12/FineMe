import { describe, expect, it } from 'vitest'
import { buildRealEstateItem, buildRealEstateLoan, buildScheduleRows } from '@/test/fixtures'
import { equitySeries, yearlyRows, type LoanTimeline } from './realEstate'

// Maison de 2 000 € (valeur détenue), prêt de 1 200 € remboursé 100 € par mois de février 2026 à janvier 2027.
const item = buildRealEstateItem()
const timeline = (fraction = 1, overrides = {}): LoanTimeline => ({
  loan: buildRealEstateLoan(overrides),
  fraction,
  rows: buildScheduleRows(1200, 12),
})

describe('immobilier dans le temps', () => {
  it('suit le capital remboursé, le restant dû et la valeur nette mois par mois', () => {
    const rows = equitySeries([item], [timeline()], 'eur')
    expect(rows[0]).toEqual({ date: '2026-01-01', value: 2000, remaining: 1200, repaid: 0, equity: 800 })
    expect(rows[1]).toEqual({ date: '2026-02-01', value: 2000, remaining: 1100, repaid: 100, equity: 900 })
    expect(rows[rows.length - 1]).toEqual({ date: '2027-01-01', value: 2000, remaining: 0, repaid: 1200, equity: 2000 })
    expect(rows).toHaveLength(13)
  })

  it('pondère chaque prêt par la part détenue et additionne les prêts', () => {
    const second: LoanTimeline = {
      loan: buildRealEstateLoan({ id: 'l-2', principal: 600 }),
      fraction: 1,
      rows: buildScheduleRows(600, 6, '2026-05-01'),
    }
    const rows = equitySeries([item], [timeline(0.5), second], 'eur')
    // Avant sa première échéance, le second prêt compte pour tout son capital.
    expect(rows[0]).toMatchObject({ remaining: 600 + 600, repaid: 0 })
    const june = rows.find((row) => row.date === '2026-06-01')
    expect(june).toMatchObject({ remaining: 0.5 * 700 + 400, repaid: 0.5 * 500 + 200 })
    // Après sa dernière échéance (octobre), il ne pèse plus rien.
    expect(rows.find((row) => row.date === '2026-11-01')).toMatchObject({ remaining: 0.5 * 200 })
  })

  it('ignore les prêts que la vue ne détient pas, et renvoie une série vide sans prêt', () => {
    expect(equitySeries([item], [timeline(0)], 'eur')).toEqual([])
    expect(equitySeries([item], [], 'eur')).toEqual([])
  })

  it('exprime les montants en pourcentage de la valeur des biens', () => {
    const [first] = equitySeries([item], [timeline()], 'pct')
    expect(first).toEqual({ date: '2026-01-01', value: 100, remaining: 60, repaid: 0, equity: 40 })
    const worthless = buildRealEstateItem({ account: { ...item.account, owned_value: 0 } })
    expect(equitySeries([worthless], [timeline()], 'pct')[0].equity).toBeNull()
  })

  it('garde une ligne par an pour le tableau', () => {
    const rows = equitySeries([item], [timeline()], 'eur')
    expect(yearlyRows(rows).map((row) => row.date)).toEqual(['2026-01-01', '2027-01-01'])
  })

  it('suit les estimations datées des biens, pondérées comme leur valeur détenue', () => {
    const estimate = (valued_on: string, value: number) => ({
      id: valued_on, account_id: 'a-maison', valued_on, value, source: 'manual' as const,
      price_per_m2: null, note: null, recorded_at: `${valued_on}T10:00:00Z`,
    })
    // Bien détenu à moitié : valeur actuelle 2 000 €, part détenue 1 000 €.
    const half = buildRealEstateItem({ account: { ...item.account, owned_value: 1000 } })
    const valuations = new Map([['a-maison', [estimate('2025-01-01', 1600), estimate('2026-03-15', 2000)]]])
    const rows = equitySeries([half], [timeline()], 'eur', valuations)
    expect(rows[0]).toMatchObject({ date: '2026-01-01', value: 800, equity: 800 - 1200 })
    expect(rows[2]).toMatchObject({ date: '2026-03-01', value: 1000 })
    expect(rows[rows.length - 1]).toMatchObject({ value: 1000, equity: 1000 })
    // Sans estimation à la date, la valeur actuelle détenue est retenue.
    const early = new Map([['a-maison', [estimate('2030-01-01', 9999)]]])
    expect(equitySeries([half], [timeline()], 'eur', early)[0].value).toBe(1000)
  })
})
