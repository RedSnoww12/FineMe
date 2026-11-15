import { describe, expect, it } from 'vitest'
import { futureValue, monthlyRate } from './compound'

describe('monthlyRate', () => {
  it('compose sur 12 mois pour retrouver le taux annuel', () => {
    expect(Math.pow(1 + monthlyRate(7), 12)).toBeCloseTo(1.07, 10)
  })
  it('vaut 0 pour un taux nul', () => {
    expect(monthlyRate(0)).toBe(0)
  })
})

describe('futureValue', () => {
  it('capitalise un capital seul au taux annuel', () => {
    expect(futureValue(1000, 0, 5, 12)).toBeCloseTo(1050, 6)
    expect(futureValue(1000, 0, 5, 120)).toBeCloseTo(1000 * Math.pow(1.05, 10), 6)
  })
  it('additionne simplement les versements à taux nul', () => {
    expect(futureValue(500, 100, 0, 24)).toBe(2900)
  })
  it('correspond à une simulation mois par mois', () => {
    const r = monthlyRate(7)
    let value = 5000
    for (let m = 0; m < 180; m++) value = value * (1 + r) + 300
    expect(futureValue(5000, 300, 7, 180)).toBeCloseTo(value, 4)
  })
  it('renvoie le capital initial à horizon nul', () => {
    expect(futureValue(1234, 100, 7, 0)).toBe(1234)
  })
})


describe('cas limites de la capitalisation', () => {
  it('arrondit le nombre de mois et ignore les durées négatives', () => {
    expect(futureValue(1000, 100, 0, 2.4)).toBe(1200)
    expect(futureValue(1000, 100, 5, -12)).toBe(1000)
  })
  it('accepte un rendement négatif', () => {
    expect(futureValue(1000, 0, -50, 12)).toBeCloseTo(500, 8)
    expect(monthlyRate(-50)).toBeLessThan(0)
  })
  it('sans capital ni versement, reste à zéro', () => {
    expect(futureValue(0, 0, 8, 240)).toBe(0)
  })
})
