// Cascade de la variation du patrimoine net sur une période : d'où vient l'écart entre le début et la fin.
import type { HistoryPoint } from '@/shared/api/types'
import { share, type Unit } from './period'

/**
 * Décomposition de la variation du patrimoine net entre deux instantanés :
 * - `performance` : variation de la plus-value latente des positions au PRU connu (null sans détail par compte) ;
 * - `debt_repayment` : baisse des dettes (négative si une dette a été contractée) ;
 * - `contributions` : le reste — versements, retraits, revalorisations manuelles et ventes.
 * Les trois composantes somment exactement à `end − start`.
 */
export type Variation = {
  start: number
  end: number
  contributions: number
  performance: number | null
  debt_repayment: number
}

function latentGain(point: HistoryPoint): number | null {
  if (point.accounts === null) return null
  return point.accounts.reduce((sum, account) => sum + (account.gain ?? 0), 0)
}

export function periodVariation(first: HistoryPoint, last: HistoryPoint): Variation {
  const startGain = latentGain(first)
  const endGain = latentGain(last)
  const performance = startGain !== null && endGain !== null ? endGain - startGain : null
  const debtRepayment = first.liabilities - last.liabilities
  const change = last.net_worth - first.net_worth
  return {
    start: first.net_worth,
    end: last.net_worth,
    contributions: change - debtRepayment - (performance ?? 0),
    performance,
    debt_repayment: debtRepayment,
  }
}

export type WaterfallStepKey = 'start' | 'contributions' | 'performance' | 'debt_repayment' | 'end'

/** Barre de la cascade : `range` = [bas, haut] de la barre flottante, `amount` = montant signé affiché. */
export type WaterfallStep = {
  key: WaterfallStepKey
  label: string
  amount: number
  range: [number, number]
  kind: 'total' | 'up' | 'down'
}

export const WATERFALL_LABEL: Record<WaterfallStepKey, string> = {
  start: 'Début de période',
  contributions: 'Apports et autres variations',
  performance: 'Performance des placements',
  debt_repayment: 'Remboursement de dettes',
  end: 'Fin de période',
}

/**
 * Barres de la cascade. En pourcentage, tout est rapporté au patrimoine net du début (null s'il est nul).
 * Sans performance connue, la barre correspondante est omise : son montant reste dans les apports.
 */
export function waterfallSteps(variation: Variation, unit: Unit): WaterfallStep[] | null {
  const scale = (value: number): number | null => (unit === 'eur' ? value : share(value, variation.start))
  const start = scale(variation.start)
  const end = scale(variation.end)
  if (start === null || end === null) return null

  const deltas: { key: WaterfallStepKey; value: number }[] = [
    { key: 'contributions', value: variation.contributions },
    ...(variation.performance !== null ? [{ key: 'performance' as const, value: variation.performance }] : []),
    { key: 'debt_repayment', value: variation.debt_repayment },
  ]
  const steps: WaterfallStep[] = [
    { key: 'start', label: WATERFALL_LABEL.start, amount: start, range: [Math.min(0, start), Math.max(0, start)], kind: 'total' },
  ]
  let running = start
  for (const delta of deltas) {
    const amount = scale(delta.value) ?? 0
    const next = running + amount
    steps.push({
      key: delta.key,
      label: WATERFALL_LABEL[delta.key],
      amount,
      range: [Math.min(running, next), Math.max(running, next)],
      kind: amount < 0 ? 'down' : 'up',
    })
    running = next
  }
  steps.push({ key: 'end', label: WATERFALL_LABEL.end, amount: end, range: [Math.min(0, end), Math.max(0, end)], kind: 'total' })
  return steps
}
