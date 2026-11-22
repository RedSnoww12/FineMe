// Profil investisseur : fourchette de matelas de sécurité et allocation indicatives. Indication générale, pas un conseil.
import type { Overview } from '@/shared/api/types'

export type Horizon = 'short' | 'medium' | 'long'
export type Risk = 'low' | 'medium' | 'high'
export type IncomeStability = 'stable' | 'moderate' | 'variable'
export type Dependents = 'none' | 'some' | 'many'

export type Profile = {
  horizon: Horizon
  risk: Risk
  income: IncomeStability
  dependents: Dependents
  homeowner: boolean
}

export const DEFAULT_PROFILE: Profile = { horizon: 'medium', risk: 'medium', income: 'stable', dependents: 'none', homeowner: false }

export const QUESTIONS = {
  horizon: {
    label: 'Horizon de placement',
    options: { short: 'Moins de 3 ans', medium: 'De 3 à 8 ans', long: 'Plus de 8 ans' },
  },
  risk: {
    label: 'Tolérance au risque',
    options: { low: 'Faible : je supporte mal les baisses', medium: 'Moyenne', high: 'Élevée : j’accepte de fortes variations' },
  },
  income: {
    label: 'Stabilité des revenus',
    options: { stable: 'Stables (salaire fixe, pension)', moderate: 'Assez stables (primes, contrat court)', variable: 'Variables (indépendant, saisonnier)' },
  },
  dependents: {
    label: 'Personnes à charge',
    options: { none: 'Aucune', some: 'Une ou deux', many: 'Trois ou plus' },
  },
} as const

export type Factor = { label: string; min: number; max: number }
export type SafetyRange = { min: number; max: number; factors: Factor[] }

/** Base de 3 à 6 mois de dépenses, augmentée selon la situation ; chaque facteur dit ce qu'il ajoute. */
export function safetyNetRange(profile: Profile): SafetyRange {
  const factors: Factor[] = [{ label: 'Base : imprévus courants (panne, santé, réparation)', min: 3, max: 6 }]
  if (profile.income === 'moderate') factors.push({ label: 'Revenus assez stables', min: 1, max: 2 })
  if (profile.income === 'variable') factors.push({ label: 'Revenus variables : creux d’activité à absorber', min: 3, max: 6 })
  if (profile.dependents === 'some') factors.push({ label: 'Une ou deux personnes à charge', min: 1, max: 2 })
  if (profile.dependents === 'many') factors.push({ label: 'Trois personnes à charge ou plus', min: 2, max: 3 })
  if (profile.homeowner) factors.push({ label: 'Propriétaire : travaux et gros entretien', min: 0, max: 1 })
  if (profile.risk === 'low') factors.push({ label: 'Faible tolérance au risque : plus de sécurité', min: 1, max: 1 })
  return {
    min: factors.reduce((sum, f) => sum + f.min, 0),
    max: factors.reduce((sum, f) => sum + f.max, 0),
    factors,
  }
}

/** Part indicative des actifs risqués (titres et crypto) dans les placements financiers, en %. */
export function targetRiskyShare(profile: Profile): number {
  const byRisk: Record<Risk, number> = { low: 25, medium: 50, high: 75 }
  const byHorizon: Record<Horizon, number> = { short: -20, medium: 0, long: 10 }
  return Math.min(90, Math.max(0, byRisk[profile.risk] + byHorizon[profile.horizon]))
}

export type Allocation = { riskyPct: number; liquidPct: number; total: number }

/** Répartition actuelle des placements financiers (hors immobilier et autres) : titres et crypto contre liquidités. */
export function currentAllocation(overview: Overview): Allocation | null {
  const value = (category: string) => overview.allocation.find((slice) => slice.category === category)?.value ?? 0
  const risky = value('investment') + value('crypto')
  const liquid = value('checking') + value('savings')
  const total = risky + liquid
  if (total <= 0) return null
  return { riskyPct: (risky / total) * 100, liquidPct: (liquid / total) * 100, total }
}

/** Situation d'un matelas visé par rapport à la fourchette recommandée. */
export function coverageVerdict(months: number | null, range: SafetyRange): 'below' | 'within' | 'above' | null {
  if (months === null) return null
  if (months < range.min) return 'below'
  if (months > range.max) return 'above'
  return 'within'
}

const STORAGE_KEY = 'fineme.profile'

/** Lit le profil enregistré sur l'appareil ; valeurs inconnues ou stockage indisponible : profil par défaut. */
export function readProfile(storage: Pick<Storage, 'getItem'> | undefined): Profile {
  try {
    const raw = storage?.getItem(STORAGE_KEY)
    if (!raw) return DEFAULT_PROFILE
    const parsed: Partial<Record<keyof Profile, unknown>> = JSON.parse(raw)
    const pick = <K extends keyof typeof QUESTIONS>(key: K): Profile[K] => {
      const value = parsed[key]
      return typeof value === 'string' && value in QUESTIONS[key].options ? (value as Profile[K]) : DEFAULT_PROFILE[key]
    }
    return {
      horizon: pick('horizon'),
      risk: pick('risk'),
      income: pick('income'),
      dependents: pick('dependents'),
      homeowner: typeof parsed.homeowner === 'boolean' ? parsed.homeowner : DEFAULT_PROFILE.homeowner,
    }
  } catch {
    return DEFAULT_PROFILE
  }
}

export function saveProfile(storage: Pick<Storage, 'setItem'> | undefined, profile: Profile) {
  try {
    storage?.setItem(STORAGE_KEY, JSON.stringify(profile))
  } catch {
    // préférence non mémorisée
  }
}
