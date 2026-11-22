import { describe, expect, it } from 'vitest'
import { buildAccountDetail, buildHolding, camille, sacha } from '@/test/fixtures'
import { assetKey, consolidate } from './consolidation'

const solo = (memberId: string, name: string) => [{ member_id: memberId, name, share: 1 }]

// Même ETF : 100 parts au PRU de 300 dans le PEA de Camille, 50 parts au PRU de 360 dans le CTO joint (50/50).
const pea = buildAccountDetail({
  id: 'pea',
  name: 'PEA Camille',
  owners: solo(camille.id, camille.name),
  holdings: [buildHolding({ id: 'h-pea' })],
})
const cto = buildAccountDetail({
  id: 'cto',
  name: 'CTO joint',
  holdings: [
    buildHolding({ id: 'h-cto', account_id: 'cto', quantity: 50, buy_price: 360, value: 16_500, invested: 18_000, gain: -1_500 }),
    buildHolding({ id: 'h-btc', account_id: 'cto', name: 'Bitcoin', isin: null, symbol: 'btc', buy_price: null, value: 500, invested: null, gain: null }),
  ],
})

describe('assetKey', () => {
  it('reconnaît un actif par ISIN, sinon symbole, sinon nom', () => {
    expect(assetKey({ isin: ' fr0000000001 ', symbol: 'X', name: 'A' })).toBe('isin:FR0000000001')
    expect(assetKey({ isin: '', symbol: 'btc', name: 'A' })).toBe('symbol:BTC')
    expect(assetKey({ isin: null, symbol: null, name: '  Fonds   Euro ' })).toBe('name:fonds euro')
  })
})

describe('consolidate', () => {
  it('réunit un ETF détenu dans deux comptes en une seule ligne', () => {
    const [etf, btc] = consolidate([pea, cto], null)
    expect(etf).toMatchObject({ key: 'isin:FR0000000001', quantity: 150, value: 49_500, invested: 48_000, gain: 1_500 })
    expect(etf?.lines.map((line) => line.accountName)).toEqual(['PEA Camille', 'CTO joint'])
    expect(btc).toMatchObject({ key: 'symbol:BTC', invested: null, gain: null, gainPct: null, averageBuyPrice: null })
  })

  it('calcule le PRU moyen pondéré par les quantités', () => {
    // (100 × 300 + 50 × 360) / 150 = 320
    expect(consolidate([pea, cto], null)[0]?.averageBuyPrice).toBe(320)
    expect(consolidate([pea, cto], null)[0]?.gainPct).toBeCloseTo(3.125)
  })

  it('répartit la valeur par membre selon les parts de chaque compte', () => {
    expect(consolidate([pea, cto], null)[0]?.members).toEqual([
      { memberId: camille.id, name: camille.name, value: 41_250 },
      { memberId: sacha.id, name: sacha.name, value: 8_250 },
    ])
  })

  it('ne garde que la part du membre affiché', () => {
    const [etf] = consolidate([pea, cto], sacha.id)
    expect(etf).toMatchObject({ quantity: 25, value: 8_250, invested: 9_000, gain: -750 })
    expect(etf?.lines.map((line) => line.accountName)).toEqual(['CTO joint'])
    expect(etf?.members).toEqual([{ memberId: sacha.id, name: sacha.name, value: 8_250 }])
    // Le PRU ne dépend pas de la part détenue.
    expect(etf?.averageBuyPrice).toBe(360)
  })

  it('ne donne pas de PRU moyen pour des lignes cotées dans des devises différentes', () => {
    const usd = buildAccountDetail({ id: 'usd', holdings: [buildHolding({ id: 'h-usd', currency: 'USD' })] })
    expect(consolidate([pea, usd], null)[0]?.averageBuyPrice).toBeNull()
  })

  it('ignore les comptes sans part détenue et un compte sans position', () => {
    expect(consolidate([pea], sacha.id)).toEqual([])
    expect(consolidate([buildAccountDetail()], null)).toEqual([])
  })
})
