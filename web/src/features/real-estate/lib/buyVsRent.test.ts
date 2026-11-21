import { describe, expect, it } from 'vitest'
import { compareBuyVsRent, type BuyVsRentInput } from './buyVsRent'

// 100 000 € empruntés à taux nul sur 10 ans (833,33 € par mois), loyer de 500 €, sans placement rémunéré.
const base: BuyVsRentInput = {
  price: 100_000,
  acquisitionFees: 0,
  downPayment: 0,
  loanRatePct: 0,
  insuranceRatePct: 0,
  loanMonths: 120,
  ownerYearlyCosts: 0,
  appreciationPct: 0,
  monthlyRent: 500,
  rentIncreasePct: 0,
  investmentReturnPct: 0,
  horizonYears: 10,
  saleFeesPct: 0,
}

describe('compareBuyVsRent', () => {
  it('place l’écart de mensualité côté locataire', () => {
    const result = compareBuyVsRent(base)
    expect(result.loanAmount).toBe(100_000)
    expect(result.monthlyLoanPayment).toBeCloseTo(833.333, 3)
    // Le locataire place 333,33 € par mois pendant 10 ans : 40 000 €. Le propriétaire détient le bien payé.
    expect(result.renterWealth).toBeCloseTo(40_000, 6)
    expect(result.buyerWealth).toBeCloseTo(100_000, 6)
    expect(result.advantage).toBeCloseTo(60_000, 6)
    // Fin de la 1re année : 10 000 € de capital remboursé contre 4 000 € placés.
    expect(result.years[0].buyerWealth).toBeCloseTo(10_000, 6)
    expect(result.years[0].renterWealth).toBeCloseTo(4000, 6)
    expect(result.breakEvenYear).toBe(1)
  })

  it('place l’écart côté propriétaire quand le loyer coûte plus cher', () => {
    const result = compareBuyVsRent({ ...base, monthlyRent: 1000 })
    // 166,67 € placés par mois la première année, en plus des 10 000 € remboursés.
    expect(result.years[0].buyerWealth).toBeCloseTo(12_000, 6)
    expect(result.years[0].renterWealth).toBe(0)
  })

  it('fait gagner la location à court terme avec un apport de 100 %', () => {
    // Le locataire garde 110 000 € ; le propriétaire paie 100 € de charges contre 500 € de loyer et place 400 €.
    const result = compareBuyVsRent({
      ...base,
      acquisitionFees: 10_000,
      downPayment: 110_000,
      ownerYearlyCosts: 1200,
      horizonYears: 4,
    })
    expect(result.loanAmount).toBe(0)
    expect(result.monthlyLoanPayment).toBe(0)
    expect(result.years.map((y) => Math.round(y.buyerWealth))).toEqual([104_800, 109_600, 114_400, 119_200])
    expect(result.renterWealth).toBe(110_000)
    expect(result.breakEvenYear).toBe(3)
  })

  it('capitalise l’apport placé par le locataire', () => {
    // Coût propriétaire = loyer (500 €) : aucun écart ; l'apport de 10 000 € rapporte 12 % sur l'année.
    const result = compareBuyVsRent({
      ...base,
      price: 10_000,
      downPayment: 10_000,
      ownerYearlyCosts: 6000,
      investmentReturnPct: 12,
      horizonYears: 1,
    })
    expect(result.renterWealth).toBeCloseTo(11_200, 6)
    expect(result.buyerWealth).toBeCloseTo(10_000, 6)
    expect(result.breakEvenYear).toBeNull()
  })

  it('déduit les frais de revente et intègre la revalorisation', () => {
    const result = compareBuyVsRent({ ...base, appreciationPct: 10, saleFeesPct: 5, horizonYears: 1 })
    // 110 000 × 95 % - 90 000 de capital restant dû.
    expect(result.buyerWealth).toBeCloseTo(110_000 * 0.95 - 90_000, 6)
  })

  it('renvoie un horizon vide sans année', () => {
    const result = compareBuyVsRent({ ...base, horizonYears: 0 })
    expect(result.years).toEqual([])
    expect(result.breakEvenYear).toBeNull()
  })
})
