import { describe, expect, it } from 'vitest'
import { buildAccountDetail, buildHolding, camille, sacha } from '@/test/fixtures'
import { cumulativeCost, portfolioFees } from './portfolioFees'

// Assurance-vie de Camille : 40 000 € à 0,6 % de frais de gestion, dont 30 000 € d'un fonds à 1,8 % de frais courants.
const lifeInsurance = buildAccountDetail({
  id: 'av',
  name: 'Assurance-vie',
  kind: 'investment',
  envelope: 'assurance_vie',
  management_fee_pct: 0.6,
  value: 40_000,
  owners: [{ member_id: camille.id, name: camille.name, share: 1 }],
  holdings: [
    buildHolding({ id: 'h-actif', name: 'Fonds actif', value: 30_000, expense_ratio: 0.018 }),
    buildHolding({ id: 'h-euro', name: 'Fonds en euros', kind: 'fonds_euro', value: 10_000, expense_ratio: null }),
  ],
})
// PEA joint (50/50) sans frais d'enveloppe : 20 000 € d'un ETF à 0,2 %.
const pea = buildAccountDetail({
  id: 'pea',
  name: 'PEA',
  kind: 'investment',
  envelope: 'pea',
  value: 20_000,
  holdings: [buildHolding({ id: 'h-etf', name: 'ETF Monde', value: 20_000, expense_ratio: 0.002 })],
})
const livret = buildAccountDetail({ id: 'livret', kind: 'savings', value: 10_000, management_fee_pct: 1 })

describe('portfolioFees', () => {
  it('additionne les frais de l’enveloppe et les frais courants des fonds, en euros par an', () => {
    const fees = portfolioFees([lifeInsurance, pea, livret], null)
    // 0,6 % × 40 000 = 240 ; 1,8 % × 30 000 = 540 ; 0,2 % × 20 000 = 40.
    expect(fees.envelopeCost).toBeCloseTo(240)
    expect(fees.fundsCost).toBeCloseTo(580)
    expect(fees.totalCost).toBeCloseTo(820)
    expect(fees.value).toBe(60_000)
  })

  it('rattache chaque frais à son compte et à sa source', () => {
    const [av, peaCost] = portfolioFees([pea, lifeInsurance], null).accounts
    expect(av).toMatchObject({ accountId: 'av', envelopePct: 0.6 })
    expect(av?.effectivePct).toBeCloseTo(1.95)
    expect(av?.envelopeCost).toBeCloseTo(240)
    expect(av?.fundsCost).toBeCloseTo(540)
    expect(peaCost).toMatchObject({ accountId: 'pea', envelopePct: null, envelopeCost: 0 })
  })

  it('classe les positions de la plus coûteuse à la moins coûteuse et compte celles sans frais connus', () => {
    const fees = portfolioFees([pea, lifeInsurance], null)
    expect(fees.funds.map((f) => f.name)).toEqual(['Fonds actif', 'ETF Monde'])
    expect(fees.funds[0]?.ratePct).toBeCloseTo(1.8)
    expect(fees.unknownFunds).toBe(1)
  })

  it('ne compte que la part du membre affiché et ignore les comptes hors titres', () => {
    const fees = portfolioFees([lifeInsurance, pea, livret], sacha.id)
    expect(fees.accounts.map((a) => a.accountId)).toEqual(['pea'])
    expect(fees.value).toBe(10_000)
    expect(fees.totalCost).toBeCloseTo(20)
  })

  it('un compte vide a un coût effectif nul', () => {
    const empty = buildAccountDetail({ id: 'vide', kind: 'investment', value: 0, management_fee_pct: 0.5 })
    expect(portfolioFees([empty], null).accounts[0]?.effectivePct).toBe(0)
  })
})

describe('cumulativeCost', () => {
  it('croît avec la durée et dépasse la somme des frais annuels (rendement perdu)', () => {
    const fees = portfolioFees([lifeInsurance], null)
    const [ten, twenty, thirty] = [10, 20, 30].map((years) => cumulativeCost(fees, years, 5))
    expect(ten).toBeGreaterThan(0)
    expect(twenty).toBeGreaterThan(ten ?? 0)
    expect(thirty).toBeGreaterThan(twenty ?? 0)
    expect(thirty).toBeGreaterThan(fees.totalCost * 30)
  })

  it('est nul sans frais ou sans encours', () => {
    expect(cumulativeCost({ value: 10_000, envelopeCost: 0, fundsCost: 0 }, 20, 5)).toBe(0)
    expect(cumulativeCost({ value: 0, envelopeCost: 0, fundsCost: 0 }, 20, 5)).toBe(0)
  })
})
