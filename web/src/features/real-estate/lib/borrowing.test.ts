import { describe, expect, it } from 'vitest'
import { borrowingCapacity, debtRatioPct } from './borrowing'
import { loanMonthlyPayment } from './loan'

describe('debtRatioPct', () => {
  it('rapporte les charges aux revenus', () => {
    expect(debtRatioPct(4000, 1000)).toBe(25)
    expect(debtRatioPct(4000, 0)).toBe(0)
  })
  it('n’est pas défini sans revenus', () => {
    expect(debtRatioPct(0, 500)).toBeNull()
  })
})

const base = { monthlyIncome: 4000, existingMonthlyDebts: 200, maxDebtRatioPct: 35, annualRatePct: 0, insuranceRatePct: 0, months: 240 }

describe('borrowingCapacity', () => {
  it('déduit les crédits en cours du plafond', () => {
    // 35 % de 4 000 € = 1 400 €, moins 200 € déjà engagés : 1 200 € par mois.
    const result = borrowingCapacity(base)
    expect(result.currentDebtRatioPct).toBe(5)
    expect(result.maxMonthlyPayment).toBeCloseTo(1200, 9)
    // Taux nul : 1 200 € × 240 mois.
    expect(result.capacity).toBeCloseTo(288_000, 6)
    expect(result.resultingDebtRatioPct).toBeCloseTo(35, 9)
  })

  it('compte l’assurance dans la mensualité', () => {
    // Par euro emprunté : 1/240 + 0,3 %/12 = 0,0044166… ⇒ 1 200 / 0,0044166… ≈ 271 698,11 €.
    const result = borrowingCapacity({ ...base, insuranceRatePct: 0.3 })
    expect(result.capacity).toBeCloseTo(271_698.11, 2)
    expect(result.monthlyInsurance).toBeCloseTo(67.92, 2)
  })

  it('inverse la formule de la mensualité avec intérêts', () => {
    const result = borrowingCapacity({ ...base, annualRatePct: 4 })
    // 200 000 € sur 20 ans à 4 % coûtent 1 211,96 € par mois ⇒ 1 200 € financent ≈ 198 025 €.
    expect(result.capacity).toBeCloseTo(198_025, -1)
    expect(loanMonthlyPayment(result.capacity, 4, 240)).toBeCloseTo(1200, 6)
  })

  it('est nulle quand les crédits en cours dépassent le plafond', () => {
    const result = borrowingCapacity({ ...base, existingMonthlyDebts: 1500 })
    expect(result.maxMonthlyPayment).toBe(0)
    expect(result.capacity).toBe(0)
    expect(result.resultingDebtRatioPct).toBeCloseTo(37.5, 9)
  })

  it('est nulle sans revenus ou sans durée', () => {
    expect(borrowingCapacity({ ...base, monthlyIncome: 0 }).capacity).toBe(0)
    expect(borrowingCapacity({ ...base, monthlyIncome: 0 }).currentDebtRatioPct).toBeNull()
    expect(borrowingCapacity({ ...base, months: 0 }).capacity).toBe(0)
  })
})
