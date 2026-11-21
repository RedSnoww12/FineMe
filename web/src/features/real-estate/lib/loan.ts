// Prêt amortissable à taux fixe. Fonctions pures.

/** Mensualité hors assurance d'un prêt amortissable à taux fixe. */
export function loanMonthlyPayment(principal: number, annualRatePct: number, months: number): number {
  if (principal <= 0 || months <= 0) return 0
  // Convention bancaire française : taux périodique proportionnel (taux annuel / 12).
  const r = annualRatePct / 100 / 12
  if (r === 0) return principal / months
  return (principal * r) / (1 - Math.pow(1 + r, -months))
}

export type LoanSummary = {
  monthlyPayment: number
  monthlyInsurance: number
  monthlyTotal: number
  totalInterest: number
  totalInsurance: number
  /** Intérêts + assurance. */
  totalCost: number
  /** Capital + intérêts + assurance. */
  totalRepaid: number
}

/** `insuranceRatePct` : taux annuel appliqué au capital initial. */
export function loanSummary(
  principal: number,
  annualRatePct: number,
  insuranceRatePct: number,
  months: number,
): LoanSummary {
  const n = Math.max(0, Math.round(months))
  const monthlyPayment = loanMonthlyPayment(principal, annualRatePct, n)
  const monthlyInsurance = n > 0 ? (Math.max(0, principal) * insuranceRatePct) / 100 / 12 : 0
  const totalInterest = Math.max(0, monthlyPayment * n - Math.max(0, principal))
  const totalInsurance = monthlyInsurance * n
  return {
    monthlyPayment,
    monthlyInsurance,
    monthlyTotal: monthlyPayment + monthlyInsurance,
    totalInterest,
    totalInsurance,
    totalCost: totalInterest + totalInsurance,
    totalRepaid: (n > 0 ? Math.max(0, principal) : 0) + totalInterest + totalInsurance,
  }
}

/** Capital restant dû après `paid` mensualités d'un prêt amortissable (borné à [0, principal]). */
export function loanRemainingBalance(principal: number, annualRatePct: number, months: number, paid: number): number {
  if (principal <= 0 || months <= 0) return 0
  const k = Math.min(Math.max(0, Math.floor(paid)), months)
  if (k >= months) return 0
  const r = annualRatePct / 100 / 12
  if (r === 0) return principal * (1 - k / months)
  const payment = loanMonthlyPayment(principal, annualRatePct, months)
  const growth = Math.pow(1 + r, k)
  return Math.max(0, principal * growth - (payment * (growth - 1)) / r)
}

