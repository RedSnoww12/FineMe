import { describe, expect, it } from 'vitest'
import { irr, npv } from './irr'
import { simulateRental, type RentalInput } from './rental'

describe('irr', () => {
  it('retrouve un rendement sur une période', () => {
    expect(irr([-100, 110])).toBeCloseTo(0.1, 9)
  })
  it('retrouve un rendement composé', () => {
    expect(irr([-100, 0, 121])).toBeCloseTo(0.1, 9)
  })
  it('résout un cas à deux flux intermédiaires', () => {
    // 100 = 50/(1+r) + 60/(1+r)² ⇒ 1/(1+r) = (-50 + √26 500) / 120 ⇒ r ≈ 6,394 %.
    expect(irr([-100, 50, 60])).toBeCloseTo(0.06394, 4)
  })
  it('gère un rendement négatif', () => {
    expect(irr([-100, 90])).toBeCloseTo(-0.1, 9)
  })
  it('n’est pas défini sans changement de signe', () => {
    expect(irr([0, 50])).toBeNull()
    expect(irr([-100, -10])).toBeNull()
    expect(irr([-100])).toBeNull()
  })
  it('annule la VAN', () => {
    const flows = [-1000, 300, 300, 300, 300]
    expect(npv(irr(flows) as number, flows)).toBeCloseTo(0, 6)
    expect(npv(0, flows)).toBe(200)
  })
})

// Prix de revient 110 000 € (100 000 + 8 000 de frais + 2 000 de travaux), apport 10 000 €,
// 100 000 € empruntés à taux nul sur 200 mois : 500 € par mois.
const base: RentalInput = {
  price: 100_000,
  acquisitionFees: 8_000,
  works: 2_000,
  downPayment: 10_000,
  loanRatePct: 0,
  insuranceRatePct: 0,
  loanMonths: 200,
  monthlyRent: 800,
  vacancyPct: 0,
  monthlyCharges: 50,
  yearlyPropertyTax: 600,
  horizonYears: 10,
  appreciationPct: 0,
  rentIndexationPct: 0,
  saleFeesPct: 0,
}

describe('simulateRental', () => {
  const result = simulateRental(base)

  it('calcule le prix de revient et le montant emprunté', () => {
    expect(result.totalCost).toBe(110_000)
    expect(result.invested).toBe(10_000)
    expect(result.loanAmount).toBe(100_000)
    expect(result.monthlyLoanPayment).toBe(500)
  })

  it('calcule les rendements brut et net sur le prix de revient', () => {
    // Brut : 9 600 / 110 000. Net : (9 600 - 600 de charges - 600 de taxe) / 110 000.
    expect(result.grossYieldPct).toBeCloseTo(8.727_272, 5)
    expect(result.netYieldPct).toBeCloseTo(7.636_363, 5)
  })

  it('calcule le cash-flow mensuel sans effort d’épargne', () => {
    // 8 400 / 12 = 700 € de loyer net, moins 500 € de mensualité.
    expect(result.monthlyCashFlow).toBeCloseTo(200, 9)
    expect(result.monthlySavingsEffort).toBe(0)
  })

  it('calcule la revente et le gain sur l’horizon', () => {
    // Revente 102 000 € (prix + travaux), capital restant dû après 120 mois : 40 000 €.
    expect(result.salePrice).toBe(102_000)
    expect(result.remainingDebtAtSale).toBeCloseTo(40_000, 6)
    expect(result.netSaleProceeds).toBeCloseTo(62_000, 6)
    expect(result.years).toHaveLength(10)
    expect(result.years[0].cashFlow).toBeCloseTo(2400, 9)
    // -10 000 + 10 × 2 400 + 62 000.
    expect(result.netGain).toBeCloseTo(76_000, 6)
    const flows = [-10_000, ...Array.from({ length: 9 }, () => 2400), 2400 + 62_000]
    expect(result.irrPct).toBeCloseTo((irr(flows) as number) * 100, 9)
  })

  it('mesure l’effort d’épargne quand le cash-flow est négatif', () => {
    // 400 € × 12 - 1 200 € = 3 600 € par an, soit 300 € par mois : 200 € manquent pour la mensualité.
    const negative = simulateRental({ ...base, monthlyRent: 400 })
    expect(negative.monthlyCashFlow).toBeCloseTo(-200, 9)
    expect(negative.monthlySavingsEffort).toBeCloseTo(200, 9)
  })

  it('applique la vacance locative aux loyers seulement', () => {
    // 800 × 12 × 75 % = 7 200, moins 1 200 de charges : 6 000 €.
    const vacant = simulateRental({ ...base, vacancyPct: 25 })
    expect(vacant.netYieldPct).toBeCloseTo((6000 / 110_000) * 100, 9)
    expect(vacant.grossYieldPct).toBeCloseTo(result.grossYieldPct as number, 9)
  })

  it('n’a pas de rendement sans loyer', () => {
    const empty = simulateRental({ ...base, monthlyRent: 0 })
    expect(empty.grossYieldPct).toBeNull()
    expect(empty.netYieldPct).toBeNull()
  })

  it('finance tout par l’apport : TRI égal à la revalorisation', () => {
    // 1 000 € investis, revendus 1 100 € un an plus tard, sans loyer ni charge.
    const cash = simulateRental({
      ...base,
      price: 1000,
      acquisitionFees: 0,
      works: 0,
      downPayment: 1000,
      monthlyRent: 0,
      monthlyCharges: 0,
      yearlyPropertyTax: 0,
      horizonYears: 1,
      appreciationPct: 10,
    })
    expect(cash.loanAmount).toBe(0)
    expect(cash.monthlyLoanPayment).toBe(0)
    expect(cash.salePrice).toBeCloseTo(1100, 9)
    expect(cash.irrPct).toBeCloseTo(10, 6)
  })

  it('plafonne l’apport au prix de revient', () => {
    const over = simulateRental({ ...base, downPayment: 500_000 })
    expect(over.invested).toBe(110_000)
    expect(over.loanAmount).toBe(0)
    expect(over.monthlyCashFlow).toBeCloseTo(700, 9)
  })

  it('déduit les frais de revente', () => {
    const fees = simulateRental({ ...base, saleFeesPct: 5 })
    expect(fees.netSaleProceeds).toBeCloseTo(102_000 * 0.95 - 40_000, 6)
  })

  it('revalorise loyers et charges chaque année', () => {
    const indexed = simulateRental({ ...base, rentIndexationPct: 2 })
    expect(indexed.years[0].netIncome).toBeCloseTo(8400, 9)
    expect(indexed.years[1].netIncome).toBeCloseTo(8400 * 1.02, 9)
  })

  it('cesse les mensualités à la fin du prêt', () => {
    // 100 000 € sur 18 mois : 12 échéances la première année, 6 la deuxième, puis plus rien.
    const short = simulateRental({ ...base, loanMonths: 18 })
    const payment = 100_000 / 18
    expect(short.years[0].loanPayments).toBeCloseTo(12 * payment, 6)
    expect(short.years[1].loanPayments).toBeCloseTo(6 * payment, 6)
    expect(short.years[2].loanPayments).toBe(0)
    expect(short.remainingDebtAtSale).toBe(0)
  })

  it('n’a pas de TRI sans apport ni flux négatif', () => {
    const noCash = simulateRental({ ...base, downPayment: 0, works: 0, acquisitionFees: 0 })
    expect(noCash.invested).toBe(0)
    expect(noCash.irrPct).toBeNull()
  })

  it('prend en compte un taux d’emprunt non nul', () => {
    const withRate = simulateRental({ ...base, loanRatePct: 4, loanMonths: 240, insuranceRatePct: 0.3 })
    // 100 000 € sur 20 ans à 4 % : 605,98 €, plus 25 € d'assurance.
    expect(withRate.monthlyLoanPayment).toBeCloseTo(630.98, 2)
    expect(withRate.monthlyCashFlow).toBeCloseTo(700 - 630.98, 2)
  })
})
