import { describe, expect, it } from 'vitest'
import { loanMonthlyPayment, loanRemainingBalance, loanSummary } from './loan'

describe('loanMonthlyPayment', () => {
  it('calcule la mensualité d’un prêt classique', () => {
    // 200 000 € sur 20 ans à 4 % : 1 211,96 €
    expect(loanMonthlyPayment(200_000, 4, 240)).toBeCloseTo(1211.96, 2)
  })
  it('divise le capital à taux nul', () => {
    expect(loanMonthlyPayment(12_000, 0, 12)).toBe(1000)
  })
  it('renvoie 0 pour une durée ou un capital nul', () => {
    expect(loanMonthlyPayment(0, 4, 240)).toBe(0)
    expect(loanMonthlyPayment(1000, 4, 0)).toBe(0)
  })
  it('amortit exactement le capital', () => {
    const payment = loanMonthlyPayment(150_000, 3.5, 300)
    let remaining = 150_000
    for (let m = 0; m < 300; m++) remaining -= payment - (remaining * 3.5) / 100 / 12
    expect(remaining).toBeCloseTo(0, 4)
  })
})

describe('loanSummary', () => {
  const summary = loanSummary(200_000, 4, 0.3, 240)
  it('calcule l’assurance sur le capital initial', () => {
    expect(summary.monthlyInsurance).toBeCloseTo(50, 8)
    expect(summary.totalInsurance).toBeCloseTo(12_000, 6)
  })
  it('totalise le coût du crédit', () => {
    expect(summary.totalInterest).toBeCloseTo(summary.monthlyPayment * 240 - 200_000, 6)
    expect(summary.totalCost).toBeCloseTo(summary.totalInterest + 12_000, 6)
    expect(summary.totalRepaid).toBeCloseTo(200_000 + summary.totalCost, 6)
    expect(summary.monthlyTotal).toBeCloseTo(summary.monthlyPayment + 50, 8)
  })
})


describe('cas limites du prêt', () => {
  it('un prêt sur un mois rembourse le capital et un mois d’intérêts', () => {
    expect(loanMonthlyPayment(1200, 12, 1)).toBeCloseTo(1212, 8)
  })
  it('sans assurance ni intérêts, le coût du crédit est nul', () => {
    const summary = loanSummary(24_000, 0, 0, 24)
    expect(summary.monthlyTotal).toBe(1000)
    expect(summary.totalCost).toBe(0)
    expect(summary.totalRepaid).toBe(24_000)
  })
  it('renvoie des zéros pour une durée nulle', () => {
    expect(loanSummary(100_000, 4, 0.3, 0)).toEqual({
      monthlyPayment: 0,
      monthlyInsurance: 0,
      monthlyTotal: 0,
      totalInterest: 0,
      totalInsurance: 0,
      totalCost: 0,
      totalRepaid: 0,
    })
  })
  it('renvoie des zéros pour un capital nul ou négatif', () => {
    expect(loanSummary(0, 4, 0.3, 240).totalRepaid).toBe(0)
    expect(loanSummary(-5000, 4, 0.3, 240).monthlyTotal).toBe(0)
  })
  it('arrondit une durée fractionnaire au mois', () => {
    expect(loanSummary(12_000, 0, 0, 11.6).monthlyPayment).toBe(1000)
  })
  it('un taux plus élevé augmente mensualité et intérêts', () => {
    const low = loanSummary(200_000, 2, 0, 300)
    const high = loanSummary(200_000, 4, 0, 300)
    expect(high.monthlyPayment).toBeGreaterThan(low.monthlyPayment)
    expect(high.totalInterest).toBeGreaterThan(low.totalInterest)
  })
})

describe('loanRemainingBalance', () => {
  it('décroît linéairement à taux nul', () => {
    expect(loanRemainingBalance(1200, 0, 12, 3)).toBe(900)
  })
  it('vaut le capital avant la première échéance et 0 à la fin', () => {
    expect(loanRemainingBalance(150_000, 3.5, 300, 0)).toBe(150_000)
    expect(loanRemainingBalance(150_000, 3.5, 300, 300)).toBe(0)
    expect(loanRemainingBalance(150_000, 3.5, 300, 400)).toBe(0)
  })
  it('suit le tableau d’amortissement', () => {
    // Première échéance de 200 000 € à 4 % sur 20 ans : 666,67 € d'intérêts, 545,29 € de capital.
    expect(loanRemainingBalance(200_000, 4, 240, 1)).toBeCloseTo(200_000 - (1211.96 - 666.67), 1)
  })
  it('est nul sans capital ni durée', () => {
    expect(loanRemainingBalance(0, 4, 240, 10)).toBe(0)
    expect(loanRemainingBalance(1000, 4, 0, 0)).toBe(0)
  })
})
