// Taux d'endettement et capacité d'emprunt. Fonctions pures.

/**
 * Taux d'endettement maximal recommandé par le Haut Conseil de stabilité financière (assurance comprise).
 * Sert d'indication dans l'interface : l'utilisateur saisit le plafond qu'il retient.
 */
export const HCSF_MAX_DEBT_RATIO_PCT = 35

/** Taux d'endettement en % : charges de crédit mensuelles / revenus mensuels. `null` sans revenus. */
export function debtRatioPct(monthlyIncome: number, monthlyDebts: number): number | null {
  if (monthlyIncome <= 0) return null
  return (Math.max(0, monthlyDebts) / monthlyIncome) * 100
}

export type BorrowingInput = {
  monthlyIncome: number
  /** Mensualités des crédits déjà en cours. */
  existingMonthlyDebts: number
  maxDebtRatioPct: number
  annualRatePct: number
  insuranceRatePct: number
  months: number
}

export type BorrowingResult = {
  currentDebtRatioPct: number | null
  /** Mensualité (assurance comprise) encore disponible sous le plafond. */
  maxMonthlyPayment: number
  /** Capital empruntable avec cette mensualité. */
  capacity: number
  monthlyInsurance: number
  /** Taux d'endettement une fois ce capital emprunté. */
  resultingDebtRatioPct: number | null
}

/** Mensualité par euro emprunté : annuité + assurance sur capital initial. */
function paymentPerEuro(annualRatePct: number, insuranceRatePct: number, months: number): number {
  const r = annualRatePct / 1200
  const annuity = r === 0 ? 1 / months : r / (1 - Math.pow(1 + r, -months))
  return annuity + Math.max(0, insuranceRatePct) / 1200
}

export function borrowingCapacity(input: BorrowingInput): BorrowingResult {
  const income = Math.max(0, input.monthlyIncome)
  const debts = Math.max(0, input.existingMonthlyDebts)
  const months = Math.round(input.months)
  const maxMonthlyPayment = Math.max(0, (income * Math.max(0, input.maxDebtRatioPct)) / 100 - debts)
  const capacity = months > 0 && maxMonthlyPayment > 0 ? maxMonthlyPayment / paymentPerEuro(input.annualRatePct, input.insuranceRatePct, months) : 0
  const monthlyPayment = capacity > 0 ? maxMonthlyPayment : 0
  return {
    currentDebtRatioPct: debtRatioPct(income, debts),
    maxMonthlyPayment,
    capacity,
    monthlyInsurance: (capacity * Math.max(0, input.insuranceRatePct)) / 1200,
    resultingDebtRatioPct: debtRatioPct(income, debts + monthlyPayment),
  }
}
