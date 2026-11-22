// Coût annuel réel du portefeuille : frais de gestion des enveloppes et frais courants des fonds, à la part de la vue.
import type { AccountDetail } from '@/shared/api/types'
import { simulateFees } from './fees'

export type FundCost = {
  holdingId: string
  name: string
  accountName: string
  value: number
  /** Frais courants annuels, en %. */
  ratePct: number
  annualCost: number
}

export type AccountCost = {
  accountId: string
  name: string
  value: number
  /** Frais de l'enveloppe, en % ; null s'ils ne sont pas renseignés. */
  envelopePct: number | null
  envelopeCost: number
  fundsCost: number
  totalCost: number
  /** Coût total rapporté à la valeur du compte, en %. */
  effectivePct: number
}

export type PortfolioFees = {
  accounts: AccountCost[]
  /** Positions dont les frais courants sont connus, de la plus coûteuse à la moins coûteuse. */
  funds: FundCost[]
  value: number
  envelopeCost: number
  fundsCost: number
  totalCost: number
  /** Positions sans frais courants renseignés : le coût réel peut être plus élevé. */
  unknownFunds: number
}

function viewFraction(owners: AccountDetail['owners'], memberId: string | null): number {
  return owners.filter((owner) => memberId === null || owner.member_id === memberId).reduce((sum, owner) => sum + owner.share, 0)
}

/** Comptes de titres et de fonds, dont la vue détient une part. Valeurs en EUR à la part de la vue. */
export function portfolioFees(details: readonly AccountDetail[], memberId: string | null): PortfolioFees {
  const accounts: AccountCost[] = []
  const funds: FundCost[] = []
  let unknownFunds = 0
  for (const detail of details) {
    const fraction = viewFraction(detail.owners, memberId)
    if (detail.kind !== 'investment' || fraction <= 0) continue
    const value = detail.value * fraction
    const envelopeCost = ((detail.management_fee_pct ?? 0) / 100) * value
    let fundsCost = 0
    for (const holding of detail.holdings) {
      if (holding.expense_ratio === null) {
        unknownFunds += 1
        continue
      }
      const holdingValue = holding.value * fraction
      const annualCost = holding.expense_ratio * holdingValue
      fundsCost += annualCost
      funds.push({
        holdingId: holding.id,
        name: holding.name,
        accountName: detail.name,
        value: holdingValue,
        ratePct: holding.expense_ratio * 100,
        annualCost,
      })
    }
    const totalCost = envelopeCost + fundsCost
    accounts.push({
      accountId: detail.id,
      name: detail.name,
      value,
      envelopePct: detail.management_fee_pct,
      envelopeCost,
      fundsCost,
      totalCost,
      effectivePct: value > 0 ? (totalCost / value) * 100 : 0,
    })
  }
  const sum = (pick: (account: AccountCost) => number) => accounts.reduce((total, account) => total + pick(account), 0)
  return {
    accounts: accounts.sort((a, b) => b.totalCost - a.totalCost),
    funds: funds.sort((a, b) => b.annualCost - a.annualCost),
    value: sum((a) => a.value),
    envelopeCost: sum((a) => a.envelopeCost),
    fundsCost: sum((a) => a.fundsCost),
    totalCost: sum((a) => a.totalCost),
    unknownFunds,
  }
}

/**
 * Coût cumulé des frais actuels sur `years` ans, sans nouveau versement : frais payés et rendement perdu sur ces frais,
 * avec l'hypothèse de rendement brut donnée.
 */
export function cumulativeCost(fees: Pick<PortfolioFees, 'value' | 'envelopeCost' | 'fundsCost'>, years: number, grossReturnPct: number): number {
  if (fees.value <= 0) return 0
  return simulateFees({
    initial: fees.value,
    monthly: 0,
    years,
    grossReturnPct,
    entryFeePct: 0,
    managementFeePct: (fees.envelopeCost / fees.value) * 100,
    terPct: (fees.fundsCost / fees.value) * 100,
    brokerage: { mode: 'fixed', value: 0 },
    performanceFeePct: 0,
  }).lostToFees
}

/** Hypothèse de rendement brut proposée pour le coût cumulé, modifiable à l'écran. */
export const DEFAULT_GROSS_RETURN_PCT = 5
/** Seuil de frais courants proposé pour signaler une position, modifiable à l'écran. */
export const DEFAULT_THRESHOLD_PCT = 1
