import { describe, expect, it } from 'vitest'
import { buildOverview } from '@/test/fixtures'
import {
  coverageVerdict,
  currentAllocation,
  DEFAULT_PROFILE,
  readProfile,
  safetyNetRange,
  saveProfile,
  targetRiskyShare,
  type Profile,
} from './profile'

const profile = (overrides: Partial<Profile> = {}): Profile => ({ ...DEFAULT_PROFILE, ...overrides })

describe('safetyNetRange', () => {
  it('part de 3 à 6 mois pour des revenus stables sans charge', () => {
    const range = safetyNetRange(profile())
    expect([range.min, range.max]).toEqual([3, 6])
    expect(range.factors).toHaveLength(1)
  })

  it('ajoute chaque facteur et l’explique', () => {
    const range = safetyNetRange(profile({ income: 'variable', dependents: 'many', homeowner: true, risk: 'low' }))
    // 3+3+2+0+1 = 9 ; 6+6+3+1+1 = 17
    expect([range.min, range.max]).toEqual([9, 17])
    expect(range.factors.map((f) => f.label)).toEqual([
      'Base : imprévus courants (panne, santé, réparation)',
      'Revenus variables : creux d’activité à absorber',
      'Trois personnes à charge ou plus',
      'Propriétaire : travaux et gros entretien',
      'Faible tolérance au risque : plus de sécurité',
    ])
  })

  it('distingue revenus assez stables et une ou deux personnes à charge', () => {
    const range = safetyNetRange(profile({ income: 'moderate', dependents: 'some' }))
    expect([range.min, range.max]).toEqual([5, 10])
  })
})

describe('targetRiskyShare', () => {
  it('dépend de la tolérance au risque et de l’horizon, bornée entre 0 et 90 %', () => {
    expect(targetRiskyShare(profile())).toBe(50)
    expect(targetRiskyShare(profile({ risk: 'low', horizon: 'short' }))).toBe(5)
    expect(targetRiskyShare(profile({ risk: 'high', horizon: 'long' }))).toBe(85)
  })
})

describe('currentAllocation', () => {
  it('compare titres et crypto aux liquidités, hors immobilier', () => {
    // Fixture : 33 000 d'investissements, 12 000 d'épargne, immobilier ignoré.
    const allocation = currentAllocation(buildOverview())
    expect(allocation?.total).toBe(45_000)
    expect(allocation?.riskyPct).toBeCloseTo(73.33, 1)
    expect(allocation?.liquidPct).toBeCloseTo(26.67, 1)
  })

  it('est inconnue sans placement financier', () => {
    expect(currentAllocation(buildOverview({ allocation: [] }))).toBeNull()
  })
})

describe('coverageVerdict', () => {
  const range = { min: 4, max: 8, factors: [] }
  it('situe la couverture visée par rapport à la fourchette, bornes incluses', () => {
    expect(coverageVerdict(3, range)).toBe('below')
    expect(coverageVerdict(4, range)).toBe('within')
    expect(coverageVerdict(8, range)).toBe('within')
    expect(coverageVerdict(12, range)).toBe('above')
    expect(coverageVerdict(null, range)).toBeNull()
  })
})

describe('readProfile / saveProfile', () => {
  const memory = () => {
    const values = new Map<string, string>()
    return { getItem: (k: string) => values.get(k) ?? null, setItem: (k: string, v: string) => void values.set(k, v) }
  }

  it('relit le profil enregistré', () => {
    const storage = memory()
    const saved = profile({ risk: 'high', homeowner: true })
    saveProfile(storage, saved)
    expect(readProfile(storage)).toEqual(saved)
  })

  it('remplace une valeur inconnue ou un contenu illisible par le profil par défaut', () => {
    const storage = memory()
    storage.setItem('fineme.profile', JSON.stringify({ risk: 'extreme', horizon: 'long', homeowner: 'oui' }))
    expect(readProfile(storage)).toEqual(profile({ horizon: 'long' }))
    storage.setItem('fineme.profile', '{')
    expect(readProfile(storage)).toEqual(DEFAULT_PROFILE)
    expect(readProfile(undefined)).toEqual(DEFAULT_PROFILE)
  })
})
