import { describe, expect, it } from 'vitest'
import { buildAccount, buildAccountDetail, buildHolding, camille, sacha } from '@/test/fixtures'
import { accountPerformance, holdingPerformance, holdsPositions, viewFraction } from './performance'

// Compte-titres 60/20 : les parts ne totalisent pas 100 %.
const cto = buildAccount({
  id: 'cto',
  name: 'Compte-titres',
  kind: 'investment',
  value: 11_000,
  invested: 10_000,
  gain: 1000,
  owners: [
    { member_id: camille.id, name: camille.name, share: 0.6 },
    { member_id: sacha.id, name: sacha.name, share: 0.2 },
  ],
})
const livret = buildAccount({ id: 'livret', kind: 'savings', invested: null, gain: null })
const crypto = buildAccount({
  id: 'crypto',
  name: 'Portefeuille crypto',
  kind: 'crypto',
  value: 1500,
  invested: 2000,
  gain: -500,
  owners: [{ member_id: sacha.id, name: sacha.name, share: 1 }],
})
const loan = buildAccount({ id: 'pret', kind: 'loan', is_liability: true, invested: 0, gain: 0 })

describe('part détenue par la vue', () => {
  it('additionne les parts pour le foyer et isole celle du membre', () => {
    expect(viewFraction(cto.owners, null)).toBeCloseTo(0.8, 9)
    expect(viewFraction(cto.owners, sacha.id)).toBe(0.2)
    expect(viewFraction(crypto.owners, camille.id)).toBe(0)
  })
})

describe('performance par compte', () => {
  it('garde les comptes au prix de revient connu, à la part détenue, du meilleur au moins bon', () => {
    const rows = accountPerformance([livret, crypto, loan, cto], null)
    expect(rows.map((r) => r.key)).toEqual(['cto', 'crypto'])
    expect(rows[0].gain).toBeCloseTo(800, 9)
    expect(rows[0].invested).toBeCloseTo(8000, 9)
    expect(rows[0].pct).toBe(10)
    expect(rows[0].detail).toBe('Banque du Canal')
    expect(rows[1]).toMatchObject({ gain: -500, pct: -25 })
  })

  it('écarte les comptes que le membre ne détient pas', () => {
    expect(accountPerformance([cto, crypto], camille.id).map((r) => [r.key, r.gain])).toEqual([['cto', 600]])
  })

  it('ne calcule pas de pourcentage sans montant investi', () => {
    const gift = buildAccount({ id: 'don', invested: 0, gain: 300, kind: 'investment' })
    expect(accountPerformance([gift], null)[0].pct).toBeNull()
  })
})

describe('performance par position', () => {
  it('liste les positions au PRU connu de tous les comptes, à la part détenue', () => {
    const details = [
      buildAccountDetail({
        ...cto,
        holdings: [
          buildHolding({ id: 'etf', name: 'Fonds Indice Monde', gain: 3000, invested: 30_000 }),
          buildHolding({ id: 'sans-pru', buy_price: null, gain: null, invested: null }),
        ],
      }),
      buildAccountDetail({
        ...crypto,
        holdings: [buildHolding({ id: 'btc', name: 'Bitcoin', gain: 4000, invested: 1000 })],
      }),
    ]
    const rows = holdingPerformance(details, sacha.id)
    expect(rows.map((r) => [r.key, r.detail])).toEqual([
      ['btc', 'Portefeuille crypto'],
      ['etf', 'Compte-titres'],
    ])
    expect(rows[0]).toMatchObject({ gain: 4000, pct: 400 })
    expect(rows[1].gain).toBeCloseTo(600, 9)
    expect(rows[1].pct).toBe(10)
  })

  it('reconnaît les comptes qui peuvent porter des positions', () => {
    expect(holdsPositions(cto)).toBe(true)
    expect(holdsPositions(loan)).toBe(false)
    expect(holdsPositions(buildAccount({ kind: 'real_estate' }))).toBe(false)
  })
})
