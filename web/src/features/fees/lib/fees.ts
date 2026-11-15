import { monthlyRate } from '@/shared/lib/compound'

export type FeeScenario = {
  initial: number
  monthly: number
  years: number
  /** Rendement annuel brut des supports, avant tout frais. */
  grossReturnPct: number
  /** Frais d'entrée prélevés sur chaque versement. */
  entryFeePct: number
  /** Frais de gestion annuels de l'enveloppe ou du contrat. */
  managementFeePct: number
  /** Frais courants annuels des fonds (TER). */
  terPct: number
  /** Frais de courtage par ordre : un ordre par versement. */
  brokerage: { mode: 'fixed' | 'percent'; value: number }
  /** Commission de surperformance : part du gain annuel net, sans seuil ni high-water mark. */
  performanceFeePct: number
}

export type FeeRow = {
  year: number
  contributed: number
  value: number
  /** Même scénario sans aucun frais. */
  baseline: number
  feesPaid: number
}

export type FeeResult = {
  rows: FeeRow[]
  contributed: number
  finalValue: number
  baselineValue: number
  /** Somme des frais effectivement prélevés. */
  totalFees: number
  /** Écart avec le scénario sans frais : frais payés + rendement perdu sur ces frais. */
  lostToFees: number
}

export function simulateFees(scenario: FeeScenario): FeeResult {
  const years = Math.max(0, Math.floor(scenario.years))
  const r = monthlyRate(scenario.grossReturnPct)
  const ongoingMonthly = (scenario.managementFeePct + scenario.terPct) / 100 / 12

  let value = 0
  let baseline = 0
  let feesPaid = 0
  let contributed = 0
  let netDepositsThisYear = 0

  const deposit = (amount: number) => {
    if (amount <= 0) return
    const entry = (amount * scenario.entryFeePct) / 100
    const afterEntry = amount - entry
    const brokerage =
      scenario.brokerage.mode === 'fixed'
        ? Math.min(Math.max(0, scenario.brokerage.value), afterEntry)
        : (afterEntry * scenario.brokerage.value) / 100
    const net = afterEntry - brokerage
    value += net
    netDepositsThisYear += net
    baseline += amount
    contributed += amount
    feesPaid += entry + brokerage
  }

  deposit(scenario.initial)
  netDepositsThisYear = 0
  const rows: FeeRow[] = [{ year: 0, contributed, value, baseline, feesPaid }]
  let valueAtYearStart = value

  for (let month = 1; month <= years * 12; month++) {
    value *= 1 + r
    baseline *= 1 + r
    const ongoing = value * ongoingMonthly
    value -= ongoing
    feesPaid += ongoing
    deposit(scenario.monthly)

    if (month % 12 === 0) {
      const yearGain = value - valueAtYearStart - netDepositsThisYear
      if (scenario.performanceFeePct > 0 && yearGain > 0) {
        const fee = (yearGain * scenario.performanceFeePct) / 100
        value -= fee
        feesPaid += fee
      }
      rows.push({ year: month / 12, contributed, value, baseline, feesPaid })
      valueAtYearStart = value
      netDepositsThisYear = 0
    }
  }

  return {
    rows,
    contributed,
    finalValue: value,
    baselineValue: baseline,
    totalFees: feesPaid,
    lostToFees: baseline - value,
  }
}
