import { describe, expect, it } from 'vitest'
import { loanMonthlyPayment, loanRemainingBalance } from './loan'
import {
  addMonthsIso,
  compareSchedules,
  monthsToRepay,
  prepaymentPenaltyCap,
  remainingAfter,
  simulateSchedule,
  type LoanTerms,
} from './schedule'

// 1 200 € sans intérêts sur 12 mois : 100 € de capital par échéance.
const flat: LoanTerms = { principal: 1200, interestRatePct: 0, insuranceRatePct: 0, durationMonths: 12, startDate: '2026-01-01' }
// 12 000 € à 12 % sur 12 mois : 1 % d'intérêts par mois.
const monthly1pct: LoanTerms = { principal: 12_000, interestRatePct: 12, insuranceRatePct: 0, durationMonths: 12, startDate: '2026-01-15' }

describe('addMonthsIso', () => {
  it('ajoute des mois en changeant d’année', () => {
    expect(addMonthsIso('2026-11-15', 2)).toBe('2027-01-15')
    expect(addMonthsIso('2026-01-01', 0)).toBe('2026-01-01')
  })
  it('ramène le jour au dernier jour du mois', () => {
    expect(addMonthsIso('2026-01-31', 1)).toBe('2026-02-28')
    expect(addMonthsIso('2024-01-31', 1)).toBe('2024-02-29')
  })
})

describe('monthsToRepay', () => {
  it('compte les mensualités à taux nul, la dernière étant partielle', () => {
    expect(monthsToRepay(1000, 0, 100)).toBe(10)
    expect(monthsToRepay(1050, 0, 100)).toBe(11)
  })
  it('retrouve la durée d’un prêt amortissable', () => {
    expect(monthsToRepay(12_000, 0.01, loanMonthlyPayment(12_000, 12, 12))).toBe(12)
  })
  it('signale une mensualité qui ne couvre pas les intérêts', () => {
    expect(monthsToRepay(12_000, 0.01, 120)).toBe(Number.POSITIVE_INFINITY)
    expect(monthsToRepay(0, 0.01, 120)).toBe(0)
  })
})

describe('simulateSchedule sans événement', () => {
  it('reproduit l’échéancier de l’API à taux nul', () => {
    const schedule = simulateSchedule(flat)
    expect(schedule.rows).toHaveLength(12)
    expect(schedule.rows[0]).toEqual({
      month: 1,
      date: '2026-02-01',
      payment: 100,
      interest: 0,
      principal: 100,
      insurance: 0,
      prepayment: 0,
      remaining: 1100,
    })
    expect(schedule.rows[11].date).toBe('2027-01-01')
    expect(schedule.rows[11].remaining).toBe(0)
    expect(schedule.endDate).toBe('2027-01-01')
    expect(schedule.totalInterest).toBe(0)
  })

  it('calcule les intérêts sur le capital restant et solde le prêt', () => {
    const schedule = simulateSchedule(monthly1pct)
    const payment = loanMonthlyPayment(12_000, 12, 12)
    expect(schedule.rows[0].interest).toBeCloseTo(120, 9)
    expect(schedule.rows[0].principal).toBeCloseTo(payment - 120, 9)
    expect(schedule.rows[5].remaining).toBeCloseTo(loanRemainingBalance(12_000, 12, 12, 6), 6)
    expect(schedule.rows[11].remaining).toBe(0)
    expect(schedule.totalInterest).toBeCloseTo(payment * 12 - 12_000, 6)
  })

  it('compte l’assurance sur le capital initial à chaque échéance', () => {
    const schedule = simulateSchedule({ ...flat, insuranceRatePct: 1 })
    expect(schedule.rows[0].insurance).toBe(1)
    expect(schedule.totalInsurance).toBe(12)
  })

  it('renvoie un échéancier vide pour un capital nul', () => {
    const schedule = simulateSchedule({ ...flat, principal: 0 })
    expect(schedule.rows).toEqual([])
    expect(schedule.finalMonthlyPayment).toBe(0)
  })
})

describe('remboursement anticipé partiel', () => {
  it('raccourcit la durée en gardant la mensualité', () => {
    // Après 2 échéances il reste 1 000 € ; 300 € remboursés laissent 700 €, soit 7 mensualités de 100 €.
    const schedule = simulateSchedule(flat, [{ kind: 'prepayment', afterMonth: 2, amount: 300, mode: 'reduce_duration' }])
    expect(schedule.months).toBe(9)
    expect(schedule.rows[1].prepayment).toBe(300)
    expect(schedule.rows[1].remaining).toBe(700)
    expect(schedule.rows[2].payment).toBe(100)
    expect(schedule.endDate).toBe('2026-10-01')
    expect(schedule.totalPrepaid).toBe(300)
    expect(schedule.finalMonthlyPayment).toBe(100)
  })

  it('baisse la mensualité en gardant la durée', () => {
    // 700 € restant sur les 10 mois restants : 70 € par mois.
    const schedule = simulateSchedule(flat, [{ kind: 'prepayment', afterMonth: 2, amount: 300, mode: 'reduce_payment' }])
    expect(schedule.months).toBe(12)
    expect(schedule.rows[2].payment).toBeCloseTo(70, 9)
    expect(schedule.finalMonthlyPayment).toBeCloseTo(70, 9)
    expect(schedule.rows[11].remaining).toBe(0)
  })

  it('termine par une échéance réduite quand le reste ne tombe pas juste', () => {
    // 1 000 € - 250 € = 750 € : 7 mensualités de 100 € puis 50 €.
    const schedule = simulateSchedule(flat, [{ kind: 'prepayment', afterMonth: 2, amount: 250, mode: 'reduce_duration' }])
    expect(schedule.months).toBe(10)
    expect(schedule.rows[9].payment).toBeCloseTo(50, 9)
  })

  it('peut intervenir avant la première échéance', () => {
    const schedule = simulateSchedule(flat, [{ kind: 'prepayment', afterMonth: 0, amount: 600, mode: 'reduce_duration' }])
    expect(schedule.initialPrepayment).toBe(600)
    expect(schedule.months).toBe(6)
  })

  it('économise des intérêts et raccourcit le prêt avec intérêts', () => {
    const base = simulateSchedule(monthly1pct)
    const shorter = simulateSchedule(monthly1pct, [{ kind: 'prepayment', afterMonth: 3, amount: 3000, mode: 'reduce_duration' }])
    const lighter = simulateSchedule(monthly1pct, [{ kind: 'prepayment', afterMonth: 3, amount: 3000, mode: 'reduce_payment' }])
    const shortened = compareSchedules(base, shorter)
    const lightened = compareSchedules(base, lighter)
    expect(shortened.monthsSaved).toBeGreaterThan(0)
    expect(shortened.monthlyPaymentChange).toBeCloseTo(0, 9)
    expect(lightened.monthsSaved).toBe(0)
    expect(lightened.monthlyPaymentChange).toBeLessThan(0)
    // Garder la mensualité rembourse plus vite, donc coûte moins d'intérêts.
    expect(shortened.interestSaved).toBeGreaterThan(lightened.interestSaved)
    expect(lightened.interestSaved).toBeGreaterThan(0)
    // Somme des flux : capital + intérêts = mensualités versées + remboursement anticipé.
    const paid = shorter.rows.reduce((sum, row) => sum + row.payment, 0) + shorter.totalPrepaid
    expect(paid).toBeCloseTo(12_000 + shorter.totalInterest, 6)
  })

  it('plafonne le remboursement au capital restant dû', () => {
    const schedule = simulateSchedule(flat, [{ kind: 'prepayment', afterMonth: 10, amount: 5000, mode: 'reduce_duration' }])
    expect(schedule.totalPrepaid).toBe(200)
    expect(schedule.months).toBe(10)
  })
})

describe('remboursement total', () => {
  it('solde le prêt après l’échéance choisie', () => {
    const schedule = simulateSchedule({ ...flat, insuranceRatePct: 1 }, [{ kind: 'full_repayment', afterMonth: 6 }])
    expect(schedule.months).toBe(6)
    expect(schedule.rows[5].prepayment).toBe(600)
    expect(schedule.rows[5].remaining).toBe(0)
    expect(schedule.totalInsurance).toBe(6)
    expect(schedule.endDate).toBe('2026-07-01')
  })

  it('ignore les événements qui suivent le solde', () => {
    const schedule = simulateSchedule(flat, [
      { kind: 'full_repayment', afterMonth: 3 },
      { kind: 'prepayment', afterMonth: 5, amount: 100, mode: 'reduce_duration' },
    ])
    expect(schedule.months).toBe(3)
    expect(schedule.totalPrepaid).toBe(900)
  })
})

describe('renégociation', () => {
  it('passe à taux nul dès le départ', () => {
    const schedule = simulateSchedule(monthly1pct, [{ kind: 'renegotiation', afterMonth: 0, interestRatePct: 0 }])
    expect(schedule.totalInterest).toBe(0)
    expect(schedule.rows[0].payment).toBeCloseTo(1000, 9)
  })

  it('recalcule la mensualité sur la durée restante', () => {
    const remaining6 = loanRemainingBalance(12_000, 12, 12, 6)
    const schedule = simulateSchedule(monthly1pct, [{ kind: 'renegotiation', afterMonth: 6, interestRatePct: 0 }])
    expect(schedule.months).toBe(12)
    expect(schedule.rows[6].interest).toBe(0)
    expect(schedule.rows[6].payment).toBeCloseTo(remaining6 / 6, 9)
    // Seuls les 6 premiers mois portent des intérêts.
    const payment = loanMonthlyPayment(12_000, 12, 12)
    expect(schedule.totalInterest).toBeCloseTo(6 * payment - (12_000 - remaining6), 6)
    expect(remainingAfter(schedule, 12_000, 6)).toBeCloseTo(remaining6, 9)
    expect(remainingAfter(schedule, 12_000, 0)).toBe(12_000)
  })
})

describe('prepaymentPenaltyCap', () => {
  it('retient six mois d’intérêts quand c’est le plus faible', () => {
    // 50 000 € à 3 % : 50 000 × 3 % / 2 = 750 € < 3 % de 100 000 € = 3 000 €.
    expect(prepaymentPenaltyCap(50_000, 100_000, 3)).toBeCloseTo(750, 9)
  })
  it('retient 3 % du capital restant dû quand c’est le plus faible', () => {
    // 100 000 € à 8 % : 4 000 € d'intérêts sur six mois > 3 000 €.
    expect(prepaymentPenaltyCap(100_000, 100_000, 8)).toBeCloseTo(3000, 9)
  })
  it('est nul sans intérêts ou sans remboursement', () => {
    expect(prepaymentPenaltyCap(10_000, 100_000, 0)).toBe(0)
    expect(prepaymentPenaltyCap(0, 100_000, 3)).toBe(0)
  })
})
