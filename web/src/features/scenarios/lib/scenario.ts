// Simulateur « Et si ? » : effet immédiat d'un événement sur le patrimoine réel, sans rien enregistrer.
import type { Account, AllocationCategory, Goal, Overview } from '@/shared/api/types'
import { ACCOUNT_KIND_PLURAL } from '@/shared/lib/labels'

export type ScenarioParams = {
  /** Variation de la valeur des investissements (titres, fonds), en % : −30 pour un krach. */
  stocksPct: number
  cryptoPct: number
  realEstatePct: number
  /** Mois sans revenus, pendant lesquels les dépenses sont prises sur les liquidités. */
  incomeLossMonths: number
  monthlyExpenses: number
  /** Achat immobilier : prix du bien et apport pris sur les liquidités ; le reste est emprunté. */
  purchasePrice: number
  downPayment: number
  /** Somme reçue, placée en épargne. */
  inheritance: number
}

export const NEUTRAL: Omit<ScenarioParams, 'monthlyExpenses'> = {
  stocksPct: 0,
  cryptoPct: 0,
  realEstatePct: 0,
  incomeLossMonths: 0,
  purchasePrice: 0,
  downPayment: 0,
  inheritance: 0,
}

export type PresetId = 'crash' | 'real_estate_drop' | 'rate_hike' | 'income_loss' | 'purchase' | 'inheritance'

/** Hypothèses de départ de chaque scénario, modifiables ensuite. */
export const PRESETS: { id: PresetId; label: string; params: Partial<ScenarioParams> }[] = [
  { id: 'crash', label: 'Krach des actions', params: { stocksPct: -30, cryptoPct: -50 } },
  { id: 'real_estate_drop', label: 'Baisse de l’immobilier', params: { realEstatePct: -15 } },
  { id: 'rate_hike', label: 'Hausse des taux', params: { stocksPct: -10, realEstatePct: -10 } },
  { id: 'income_loss', label: 'Perte de revenus', params: { incomeLossMonths: 6 } },
  { id: 'purchase', label: 'Achat immobilier', params: { purchasePrice: 250_000, downPayment: 25_000 } },
  { id: 'inheritance', label: 'Héritage', params: { inheritance: 50_000 } },
]

const CATEGORIES: AllocationCategory[] = ['checking', 'savings', 'investment', 'crypto', 'real_estate', 'other']

type Values = Record<AllocationCategory, number>
type BeforeAfter = { before: number; after: number }

export type ScenarioResult = {
  categories: ({ category: AllocationCategory; label: string } & BeforeAfter)[]
  grossAssets: BeforeAfter
  liabilities: BeforeAfter
  netWorth: BeforeAfter
  /** Dépenses et apport non couverts par les liquidités (comptes courants et épargne). */
  shortfall: number
  /** Mois de dépenses couverts par les liquidités ; null sans dépenses mensuelles. */
  coverageMonths: { before: number; after: number } | null
  /** Rapport après / avant de chaque catégorie (1 quand elle était vide). */
  ratios: Values
}

/** Une variation ne fait jamais passer une valeur sous zéro. */
function shock(value: number, pct: number): number {
  return value * Math.max(0, 1 + pct / 100)
}

function nonNegative(value: number): number {
  return Number.isFinite(value) ? Math.max(0, value) : 0
}

export function applyScenario(overview: Overview, params: ScenarioParams): ScenarioResult {
  const before = Object.fromEntries(CATEGORIES.map((c) => [c, 0])) as Values
  for (const slice of overview.allocation) before[slice.category] += slice.value

  const purchase = nonNegative(params.purchasePrice)
  const downPayment = Math.min(nonNegative(params.downPayment), purchase)
  const spending = nonNegative(params.incomeLossMonths) * nonNegative(params.monthlyExpenses) + downPayment

  const after: Values = {
    ...before,
    investment: shock(before.investment, params.stocksPct),
    crypto: shock(before.crypto, params.cryptoPct),
    real_estate: shock(before.real_estate, params.realEstatePct) + purchase,
    savings: before.savings + nonNegative(params.inheritance),
  }
  // Les dépenses sont prises sur les comptes courants, puis sur l'épargne.
  const fromChecking = Math.min(spending, after.checking)
  const fromSavings = Math.min(spending - fromChecking, after.savings)
  after.checking -= fromChecking
  after.savings -= fromSavings

  const sum = (values: Values) => CATEGORIES.reduce((total, c) => total + values[c], 0)
  const grossAssets = { before: sum(before), after: sum(after) }
  const liabilities = { before: overview.liabilities, after: overview.liabilities + purchase - downPayment }
  const expenses = nonNegative(params.monthlyExpenses)
  const liquidity = (values: Values) => values.checking + values.savings

  return {
    categories: CATEGORIES.filter((c) => before[c] !== 0 || after[c] !== 0).map((c) => ({
      category: c,
      label: ACCOUNT_KIND_PLURAL[c],
      before: before[c],
      after: after[c],
    })),
    grossAssets,
    liabilities,
    netWorth: { before: grossAssets.before - liabilities.before, after: grossAssets.after - liabilities.after },
    shortfall: spending - fromChecking - fromSavings,
    coverageMonths: expenses > 0 ? { before: liquidity(before) / expenses, after: liquidity(after) / expenses } : null,
    ratios: Object.fromEntries(CATEGORIES.map((c) => [c, before[c] > 0 ? after[c] / before[c] : 1])) as Values,
  }
}

export type GoalImpact = { goal: Goal; before: number; after: number; pctBefore: number; pctAfter: number }

/**
 * Avancement des objectifs après le scénario : chaque compte lié varie comme sa catégorie. Le montant actuel de
 * l'objectif est ajusté de la variation moyenne de ses comptes, pondérée par leur valeur.
 */
export function goalImpacts(goals: readonly Goal[], accounts: readonly Account[], ratios: Values): GoalImpact[] {
  const byId = new Map(accounts.map((account) => [account.id, account]))
  return goals.map((goal) => {
    const linked = goal.account_ids.flatMap((id) => {
      const account = byId.get(id)
      return account && account.kind !== 'loan' ? [account] : []
    })
    const value = linked.reduce((sum, account) => sum + account.value, 0)
    const shocked = linked.reduce((sum, account) => sum + account.value * ratios[account.kind as AllocationCategory], 0)
    const before = goal.progress.current_amount
    const after = value > 0 ? before * (shocked / value) : before
    const target = goal.progress.target_amount
    return {
      goal,
      before,
      after,
      pctBefore: target > 0 ? (before / target) * 100 : 0,
      pctAfter: target > 0 ? (after / target) * 100 : 0,
    }
  })
}

/** Dépenses mensuelles proposées : celles du premier matelas de sécurité, sinon 0 (à saisir). */
export function defaultMonthlyExpenses(goals: readonly Goal[]): number {
  return goals.find((goal) => goal.kind === 'safety_net' && goal.monthly_expenses)?.monthly_expenses ?? 0
}
