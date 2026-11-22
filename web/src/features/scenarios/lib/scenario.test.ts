import { describe, expect, it } from 'vitest'
import { buildAccount, buildGoal, buildOverview } from '@/test/fixtures'
import { applyScenario, defaultMonthlyExpenses, goalImpacts, NEUTRAL, PRESETS, type ScenarioParams } from './scenario'

// Actifs : 2 000 courant, 10 000 épargne, 50 000 titres, 4 000 crypto, 200 000 immobilier ; 100 000 de dettes.
const overview = buildOverview({
  liabilities: 100_000,
  allocation: [
    { category: 'checking', label: 'Comptes courants', value: 2_000, pct: 0 },
    { category: 'savings', label: 'Épargne', value: 10_000, pct: 0 },
    { category: 'investment', label: 'Investissements', value: 50_000, pct: 0 },
    { category: 'crypto', label: 'Crypto', value: 4_000, pct: 0 },
    { category: 'real_estate', label: 'Immobilier', value: 200_000, pct: 0 },
  ],
})

const params = (overrides: Partial<ScenarioParams> = {}): ScenarioParams => ({ ...NEUTRAL, monthlyExpenses: 2_000, ...overrides })

describe('applyScenario', () => {
  it('ne change rien sans hypothèse', () => {
    const result = applyScenario(overview, params())
    expect(result.netWorth).toEqual({ before: 166_000, after: 166_000 })
    expect(result.shortfall).toBe(0)
    expect(result.coverageMonths).toEqual({ before: 6, after: 6 })
  })

  it('un krach de 30 % ne touche que les investissements', () => {
    const result = applyScenario(overview, params({ stocksPct: -30 }))
    expect(result.categories.find((c) => c.category === 'investment')).toMatchObject({ before: 50_000, after: 35_000 })
    expect(result.netWorth.after).toBe(151_000)
    expect(result.liabilities.after).toBe(100_000)
    expect(result.ratios.investment).toBeCloseTo(0.7)
    expect(result.ratios.savings).toBe(1)
  })

  it('une baisse au-delà de 100 % ramène la valeur à zéro, jamais en dessous', () => {
    const result = applyScenario(overview, params({ cryptoPct: -150 }))
    expect(result.categories.find((c) => c.category === 'crypto')?.after).toBe(0)
  })

  it('une perte de revenus vide les comptes courants puis l’épargne et chiffre le manque', () => {
    const partial = applyScenario(overview, params({ incomeLossMonths: 3 }))
    expect(partial.categories.find((c) => c.category === 'checking')?.after).toBe(0)
    expect(partial.categories.find((c) => c.category === 'savings')?.after).toBe(6_000)
    expect(partial.coverageMonths?.after).toBe(3)

    const beyond = applyScenario(overview, params({ incomeLossMonths: 8 }))
    expect(beyond.shortfall).toBe(4_000)
    expect(beyond.coverageMonths?.after).toBe(0)
  })

  it('un achat immobilier ajoute le bien, le prêt et retire l’apport, sans changer le patrimoine net', () => {
    const result = applyScenario(overview, params({ purchasePrice: 150_000, downPayment: 12_000 }))
    expect(result.categories.find((c) => c.category === 'real_estate')?.after).toBe(350_000)
    expect(result.liabilities.after).toBe(238_000)
    expect(result.netWorth.after).toBe(result.netWorth.before)
    expect(result.coverageMonths?.after).toBe(0)
  })

  it('un apport supérieur au prix est plafonné au prix', () => {
    const result = applyScenario(overview, params({ purchasePrice: 1_000, downPayment: 5_000 }))
    expect(result.liabilities.after).toBe(100_000)
    expect(result.categories.find((c) => c.category === 'checking')?.after).toBe(1_000)
  })

  it('un héritage va en épargne et fait apparaître une catégorie vide', () => {
    const empty = buildOverview({ liabilities: 0, allocation: [] })
    const result = applyScenario(empty, params({ inheritance: 30_000 }))
    expect(result.categories).toEqual([{ category: 'savings', label: 'Épargne', before: 0, after: 30_000 }])
    expect(result.ratios.savings).toBe(1)
  })

  it('sans dépenses mensuelles, le nombre de mois couverts n’est pas calculé', () => {
    expect(applyScenario(overview, params({ monthlyExpenses: 0 })).coverageMonths).toBeNull()
  })

  it('ignore les saisies négatives ou non finies', () => {
    const result = applyScenario(overview, params({ incomeLossMonths: -2, inheritance: Number.NaN, purchasePrice: -10 }))
    expect(result.netWorth.after).toBe(result.netWorth.before)
    expect(result.liabilities.after).toBe(100_000)
  })

  it('chaque scénario prédéfini a un effet', () => {
    for (const preset of PRESETS) {
      const result = applyScenario(overview, params(preset.params))
      const changed = result.categories.some((c) => c.after !== c.before) || result.liabilities.after !== result.liabilities.before
      expect(changed, preset.id).toBe(true)
    }
  })
})

describe('goalImpacts', () => {
  const pea = buildAccount({ id: 'pea', kind: 'investment', value: 30_000 })
  const livret = buildAccount({ id: 'livret', kind: 'savings', value: 10_000 })
  const ratios = applyScenario(overview, params({ stocksPct: -30 })).ratios

  it('ajuste l’objectif selon la variation de ses comptes, pondérée par leur valeur', () => {
    const goal = buildGoal({
      account_ids: ['pea', 'livret'],
      progress: { ...buildGoal().progress, current_amount: 40_000, target_amount: 80_000 },
    })
    const [impact] = goalImpacts([goal], [pea, livret], ratios)
    // 30 000 × 0,7 + 10 000 = 31 000 sur 40 000 : −22,5 %.
    expect(impact?.after).toBeCloseTo(31_000)
    expect(impact?.pctBefore).toBe(50)
    expect(impact?.pctAfter).toBeCloseTo(38.75)
  })

  it('laisse inchangé un objectif sans compte connu', () => {
    const goal = buildGoal({ account_ids: ['inconnu'] })
    const [impact] = goalImpacts([goal], [pea], ratios)
    expect(impact?.after).toBe(impact?.before)
  })

  it('un objectif sans cible a un avancement nul', () => {
    const goal = buildGoal({ account_ids: [], progress: { ...buildGoal().progress, target_amount: 0 } })
    expect(goalImpacts([goal], [], ratios)[0]?.pctAfter).toBe(0)
  })
})

describe('defaultMonthlyExpenses', () => {
  it('reprend les dépenses du matelas de sécurité', () => {
    const safety = buildGoal({ kind: 'safety_net', monthly_expenses: 2_500 })
    expect(defaultMonthlyExpenses([buildGoal(), safety])).toBe(2_500)
    expect(defaultMonthlyExpenses([buildGoal()])).toBe(0)
  })
})
