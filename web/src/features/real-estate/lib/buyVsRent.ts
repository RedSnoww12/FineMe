// Comparaison acheter / louer sur un horizon donné, avant impôts. Fonctions pures.
// Les deux ménages dépensent chaque mois le même budget : le locataire place l'apport qu'il n'a pas versé
// et, chaque mois, l'écart entre le coût du propriétaire et son loyer ; si le loyer coûte plus cher,
// c'est le propriétaire qui place la différence.

import { loanRemainingBalance, loanSummary } from './loan'

export type BuyVsRentInput = {
  price: number
  acquisitionFees: number
  downPayment: number
  loanRatePct: number
  insuranceRatePct: number
  loanMonths: number
  /** Taxe foncière, charges de copropriété, entretien. */
  ownerYearlyCosts: number
  appreciationPct: number
  monthlyRent: number
  /** Hausse annuelle des loyers et des charges du propriétaire. */
  rentIncreasePct: number
  /** Rendement annuel des placements. */
  investmentReturnPct: number
  horizonYears: number
  saleFeesPct: number
}

export type BuyVsRentYear = { year: number; buyerWealth: number; renterWealth: number }

export type BuyVsRentResult = {
  loanAmount: number
  monthlyLoanPayment: number
  years: BuyVsRentYear[]
  buyerWealth: number
  renterWealth: number
  /** Positif : acheter enrichit davantage. */
  advantage: number
  /** Première année où acheter devient plus avantageux ; `null` si jamais sur l'horizon. */
  breakEvenYear: number | null
}

export function compareBuyVsRent(input: BuyVsRentInput): BuyVsRentResult {
  const totalCost = Math.max(0, input.price) + Math.max(0, input.acquisitionFees)
  const downPayment = Math.min(Math.max(0, input.downPayment), totalCost)
  const loanAmount = totalCost - downPayment
  const loanMonths = Math.max(0, Math.round(input.loanMonths))
  const loan = loanSummary(loanAmount, input.loanRatePct, input.insuranceRatePct, loanMonths)
  const monthlyReturn = Math.pow(1 + input.investmentReturnPct / 100, 1 / 12) - 1
  const horizon = Math.max(0, Math.round(input.horizonYears))

  let renterPortfolio = downPayment
  let buyerPortfolio = 0
  const years: BuyVsRentYear[] = []
  for (let year = 1; year <= horizon; year++) {
    const indexation = Math.pow(1 + input.rentIncreasePct / 100, year - 1)
    const rent = Math.max(0, input.monthlyRent) * indexation
    const ownerCosts = (Math.max(0, input.ownerYearlyCosts) / 12) * indexation
    for (let m = 1; m <= 12; m++) {
      const month = 12 * (year - 1) + m
      const buyerCost = (month <= loanMonths ? loan.monthlyTotal : 0) + ownerCosts
      renterPortfolio *= 1 + monthlyReturn
      buyerPortfolio *= 1 + monthlyReturn
      const gap = buyerCost - rent
      if (gap > 0) renterPortfolio += gap
      else buyerPortfolio -= gap
    }
    const value = Math.max(0, input.price) * Math.pow(1 + input.appreciationPct / 100, year)
    const debt = loanMonths > 0 ? loanRemainingBalance(loanAmount, input.loanRatePct, loanMonths, 12 * year) : loanAmount
    const buyerWealth = value * (1 - Math.max(0, input.saleFeesPct) / 100) - debt + buyerPortfolio
    years.push({ year, buyerWealth, renterWealth: renterPortfolio })
  }

  const last = years.at(-1)
  const buyerWealth = last?.buyerWealth ?? 0
  const renterWealth = last?.renterWealth ?? downPayment
  return {
    loanAmount,
    monthlyLoanPayment: loan.monthlyTotal,
    years,
    buyerWealth,
    renterWealth,
    advantage: buyerWealth - renterWealth,
    breakEvenYear: years.find((y) => y.buyerWealth >= y.renterWealth)?.year ?? null,
  }
}
