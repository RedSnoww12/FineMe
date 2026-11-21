// Échéancier d'un prêt à taux fixe avec remboursements anticipés et renégociation. Fonctions pures.
// Mêmes conventions que l'API (fineme-domain/loan.rs) : taux mensuel = taux annuel / 12, assurance
// calculée sur le capital initial, première échéance un mois après la date de départ, dernière échéance
// qui solde exactement le capital.

import { loanMonthlyPayment } from './loan'

export type LoanTerms = {
  principal: number
  interestRatePct: number
  insuranceRatePct: number
  durationMonths: number
  /** Date ISO `AAAA-MM-JJ` de départ du prêt. */
  startDate: string
}

/** Après un remboursement partiel : garder la mensualité (durée raccourcie) ou la durée (mensualité baissée). */
export type PrepaymentMode = 'reduce_duration' | 'reduce_payment'

/** Les événements s'appliquent juste après l'échéance `afterMonth` (0 = avant la première échéance). */
export type LoanEvent =
  | { kind: 'prepayment'; afterMonth: number; amount: number; mode: PrepaymentMode }
  | { kind: 'full_repayment'; afterMonth: number }
  | { kind: 'renegotiation'; afterMonth: number; interestRatePct: number }

export type SimulatedRow = {
  month: number
  date: string
  payment: number
  interest: number
  principal: number
  insurance: number
  /** Remboursement anticipé versé juste après cette échéance. */
  prepayment: number
  remaining: number
}

export type SimulatedSchedule = {
  rows: SimulatedRow[]
  /** Remboursement anticipé versé avant la première échéance. */
  initialPrepayment: number
  totalInterest: number
  totalInsurance: number
  totalPrepaid: number
  /** Mensualité hors assurance en vigueur après le dernier événement. */
  finalMonthlyPayment: number
  endDate: string
  months: number
}

const EPSILON = 1e-6

/** Ajoute des mois à une date ISO, en ramenant le jour au dernier jour du mois si besoin (comme l'API). */
export function addMonthsIso(iso: string, months: number): string {
  const [y, m, d] = iso.split('-').map(Number)
  const total = y * 12 + (m - 1) + months
  const year = Math.floor(total / 12)
  const month = total - year * 12
  const lastDay = new Date(Date.UTC(year, month + 1, 0)).getUTCDate()
  const day = Math.min(d, lastDay)
  return `${String(year).padStart(4, '0')}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`
}

/** Nombre de mensualités `payment` nécessaires pour solder `balance` au taux mensuel `monthlyRate`. */
export function monthsToRepay(balance: number, monthlyRate: number, payment: number): number {
  if (balance <= EPSILON) return 0
  if (payment <= 0) return Number.POSITIVE_INFINITY
  if (monthlyRate === 0) return Math.ceil(balance / payment - EPSILON)
  const ratio = (balance * monthlyRate) / payment
  if (ratio >= 1) return Number.POSITIVE_INFINITY
  return Math.ceil(-Math.log(1 - ratio) / Math.log(1 + monthlyRate) - EPSILON)
}

type State = { remaining: number; ratePct: number; payment: number; monthsLeft: number }

function applyEvent(state: State, event: LoanEvent): { state: State; prepaid: number } {
  switch (event.kind) {
    case 'full_repayment':
      return { state: { ...state, remaining: 0, monthsLeft: 0 }, prepaid: state.remaining }
    case 'prepayment': {
      const prepaid = Math.min(Math.max(0, event.amount), state.remaining)
      const remaining = state.remaining - prepaid
      if (remaining <= EPSILON) return { state: { ...state, remaining: 0, monthsLeft: 0 }, prepaid }
      if (event.mode === 'reduce_payment') {
        return { state: { ...state, remaining, payment: loanMonthlyPayment(remaining, state.ratePct, state.monthsLeft) }, prepaid }
      }
      const monthsLeft = Math.min(state.monthsLeft, monthsToRepay(remaining, state.ratePct / 1200, state.payment))
      return { state: { ...state, remaining, monthsLeft }, prepaid }
    }
    case 'renegotiation': {
      // La durée restante est conservée : la mensualité est recalculée au nouveau taux.
      const ratePct = Math.max(0, event.interestRatePct)
      return { state: { ...state, ratePct, payment: loanMonthlyPayment(state.remaining, ratePct, state.monthsLeft) }, prepaid: 0 }
    }
  }
}

function applyEventsAt(state: State, events: LoanEvent[], month: number): { state: State; prepaid: number } {
  let current = state
  let prepaid = 0
  for (const event of events) {
    if (event.afterMonth !== month || current.monthsLeft === 0) continue
    const result = applyEvent(current, event)
    current = result.state
    prepaid += result.prepaid
  }
  return { state: current, prepaid }
}

/** Échéancier complet. Sans événement, il reproduit celui de l'API. */
export function simulateSchedule(terms: LoanTerms, events: LoanEvent[] = []): SimulatedSchedule {
  const n = Math.max(0, Math.round(terms.durationMonths))
  const principal = Math.max(0, terms.principal)
  const insurance = n > 0 ? (principal * Math.max(0, terms.insuranceRatePct)) / 1200 : 0
  const ratePct = Math.max(0, terms.interestRatePct)
  const initial = applyEventsAt(
    { remaining: principal, ratePct, payment: loanMonthlyPayment(principal, ratePct, n), monthsLeft: principal > 0 ? n : 0 },
    events,
    0,
  )
  let state = initial.state

  const rows: SimulatedRow[] = []
  for (let month = 1; state.monthsLeft > 0 && state.remaining > EPSILON; month++) {
    const interest = (state.remaining * state.ratePct) / 1200
    const repaid = state.monthsLeft === 1 ? state.remaining : Math.min(state.payment - interest, state.remaining)
    state = { ...state, remaining: Math.max(0, state.remaining - repaid), monthsLeft: state.monthsLeft - 1 }
    const after = applyEventsAt(state, events, month)
    state = after.state
    rows.push({
      month,
      date: addMonthsIso(terms.startDate, month),
      payment: repaid + interest,
      interest,
      principal: repaid,
      insurance,
      prepayment: after.prepaid,
      remaining: state.remaining,
    })
  }

  const months = rows.length
  return {
    rows,
    initialPrepayment: initial.prepaid,
    totalInterest: rows.reduce((sum, row) => sum + row.interest, 0),
    totalInsurance: insurance * months,
    totalPrepaid: initial.prepaid + rows.reduce((sum, row) => sum + row.prepayment, 0),
    finalMonthlyPayment: months > 0 ? state.payment : 0,
    endDate: addMonthsIso(terms.startDate, months),
    months,
  }
}

/** Capital restant dû juste après l'échéance `month` (0 = capital initial). */
export function remainingAfter(schedule: SimulatedSchedule, principal: number, month: number): number {
  if (month <= 0) return principal
  return schedule.rows.find((row) => row.month === month)?.remaining ?? 0
}

/**
 * Plafond légal des indemnités de remboursement anticipé d'un crédit immobilier (Code de la consommation,
 * art. R313-25) : le plus faible de six mois d'intérêts sur le capital remboursé et de 3 % du capital
 * restant dû avant remboursement. Le contrat peut prévoir moins, voire rien.
 */
export const PREPAYMENT_PENALTY_CAP_PCT = 3
export const PREPAYMENT_PENALTY_CAP_MONTHS = 6

export function prepaymentPenaltyCap(amount: number, remainingBefore: number, annualRatePct: number): number {
  if (amount <= 0 || remainingBefore <= 0) return 0
  const sixMonthsInterest = (Math.min(amount, remainingBefore) * annualRatePct * PREPAYMENT_PENALTY_CAP_MONTHS) / 1200
  const capitalCap = (remainingBefore * PREPAYMENT_PENALTY_CAP_PCT) / 100
  return Math.min(sixMonthsInterest, capitalCap)
}

export type ScheduleComparison = {
  interestSaved: number
  insuranceSaved: number
  monthsSaved: number
  monthlyPaymentChange: number
}

export function compareSchedules(base: SimulatedSchedule, simulated: SimulatedSchedule): ScheduleComparison {
  return {
    interestSaved: base.totalInterest - simulated.totalInterest,
    insuranceSaved: base.totalInsurance - simulated.totalInsurance,
    monthsSaved: base.months - simulated.months,
    monthlyPaymentChange: simulated.finalMonthlyPayment - base.finalMonthlyPayment,
  }
}
