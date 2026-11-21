// Simulateur d'investissement locatif, avant impôts. Fonctions pures.

import { irr } from './irr'
import { loanRemainingBalance, loanSummary } from './loan'

export type RentalInput = {
  price: number
  /** Frais de notaire, d'agence et de garantie. */
  acquisitionFees: number
  works: number
  /** Apport personnel : il finance d'abord les frais, le reste est emprunté. */
  downPayment: number
  loanRatePct: number
  insuranceRatePct: number
  loanMonths: number
  monthlyRent: number
  /** Part de l'année sans locataire, en %. */
  vacancyPct: number
  /** Charges non récupérables, gestion, assurance propriétaire. */
  monthlyCharges: number
  yearlyPropertyTax: number
  horizonYears: number
  /** Revalorisation annuelle du bien. */
  appreciationPct: number
  /** Revalorisation annuelle des loyers et des charges. */
  rentIndexationPct: number
  /** Frais de revente, en % du prix de vente. */
  saleFeesPct: number
}

export type RentalYear = {
  year: number
  netIncome: number
  loanPayments: number
  cashFlow: number
  remainingDebt: number
}

export type RentalResult = {
  totalCost: number
  invested: number
  loanAmount: number
  /** Mensualité assurance comprise. */
  monthlyLoanPayment: number
  /** Rendements sur le prix de revient (même base que l'API) ; `null` sans loyer. */
  grossYieldPct: number | null
  netYieldPct: number | null
  /** Cash-flow mensuel de la première année : loyers nets de vacance et de charges, moins la mensualité. */
  monthlyCashFlow: number
  /** Ce qu'il faut ajouter chaque mois de sa poche (0 si le cash-flow est positif). */
  monthlySavingsEffort: number
  years: RentalYear[]
  salePrice: number
  remainingDebtAtSale: number
  /** Prix de vente moins frais de revente et capital restant dû. */
  netSaleProceeds: number
  /** Apport déduit : somme des cash-flows et du produit net de la revente. */
  netGain: number
  /** Taux de rendement interne annuel de l'apport, en % ; `null` s'il n'est pas défini. */
  irrPct: number | null
}

const positive = (value: number) => Math.max(0, value)

/** Nombre d'échéances du prêt qui tombent pendant l'année `year` (1 = première année). */
function paymentsInYear(year: number, loanMonths: number): number {
  return Math.max(0, Math.min(12, loanMonths - 12 * (year - 1)))
}

export function simulateRental(input: RentalInput): RentalResult {
  const totalCost = positive(input.price) + positive(input.acquisitionFees) + positive(input.works)
  const invested = Math.min(positive(input.downPayment), totalCost)
  const loanAmount = totalCost - invested
  const loanMonths = Math.max(0, Math.round(input.loanMonths))
  const loan = loanSummary(loanAmount, input.loanRatePct, input.insuranceRatePct, loanMonths)
  const horizon = Math.max(0, Math.round(input.horizonYears))
  const occupancy = 1 - Math.min(100, positive(input.vacancyPct)) / 100

  const yearlyNetIncome = (year: number) => {
    const indexation = Math.pow(1 + input.rentIndexationPct / 100, year - 1)
    const rent = positive(input.monthlyRent) * 12 * occupancy
    const charges = positive(input.monthlyCharges) * 12 + positive(input.yearlyPropertyTax)
    return (rent - charges) * indexation
  }
  const remainingDebt = (months: number) =>
    loanMonths > 0 ? loanRemainingBalance(loanAmount, input.loanRatePct, loanMonths, months) : loanAmount

  const years: RentalYear[] = Array.from({ length: horizon }, (_, i) => {
    const year = i + 1
    const netIncome = yearlyNetIncome(year)
    const loanPayments = loan.monthlyTotal * paymentsInYear(year, loanMonths)
    return { year, netIncome, loanPayments, cashFlow: netIncome - loanPayments, remainingDebt: remainingDebt(12 * year) }
  })

  const rent = positive(input.monthlyRent) * 12
  const grossYieldPct = rent > 0 && totalCost > 0 ? (rent / totalCost) * 100 : null
  const netYieldPct = rent > 0 && totalCost > 0 ? (yearlyNetIncome(1) / totalCost) * 100 : null
  const monthlyCashFlow = yearlyNetIncome(1) / 12 - (loanMonths > 0 ? loan.monthlyTotal : 0)

  const salePrice = (positive(input.price) + positive(input.works)) * Math.pow(1 + input.appreciationPct / 100, horizon)
  const remainingDebtAtSale = remainingDebt(12 * horizon)
  const netSaleProceeds = salePrice * (1 - positive(input.saleFeesPct) / 100) - remainingDebtAtSale

  const flows = [-invested, ...years.map((y) => y.cashFlow)]
  if (horizon > 0) flows[horizon] += netSaleProceeds
  const rate = horizon > 0 ? irr(flows) : null

  return {
    totalCost,
    invested,
    loanAmount,
    monthlyLoanPayment: loan.monthlyTotal,
    grossYieldPct,
    netYieldPct,
    monthlyCashFlow,
    monthlySavingsEffort: positive(-monthlyCashFlow),
    years,
    salePrice,
    remainingDebtAtSale,
    netSaleProceeds,
    netGain: flows.reduce((sum, flow) => sum + flow, 0),
    irrPct: rate === null ? null : rate * 100,
  }
}
