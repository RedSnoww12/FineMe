import type {
  AccountKind,
  AllocationCategory,
  BuildingType,
  GoalStatus,
  HoldingKind,
  PropertyCategory,
} from '@/shared/api/types'

export const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  checking: 'Compte courant',
  savings: 'Épargne',
  investment: 'Investissement',
  crypto: 'Crypto',
  real_estate: 'Immobilier',
  loan: 'Prêt',
  other: 'Autre',
}

export const ACCOUNT_KIND_PLURAL: Record<AccountKind, string> = {
  checking: 'Comptes courants',
  savings: 'Épargne',
  investment: 'Investissements',
  crypto: 'Crypto',
  real_estate: 'Immobilier',
  loan: 'Prêts',
  other: 'Autres',
}

export const ACCOUNT_KIND_ORDER: AccountKind[] = [
  'checking',
  'savings',
  'investment',
  'crypto',
  'real_estate',
  'other',
  'loan',
]

export const ENVELOPES: { value: string; label: string }[] = [
  { value: 'pea', label: 'PEA' },
  { value: 'pea_pme', label: 'PEA-PME' },
  { value: 'cto', label: 'Compte-titres (CTO)' },
  { value: 'assurance_vie', label: 'Assurance-vie' },
  { value: 'per', label: 'PER' },
  { value: 'livret', label: 'Livret' },
  { value: 'pee', label: 'PEE' },
]

export function envelopeLabel(envelope: string | null | undefined): string {
  if (!envelope) return 'Sans enveloppe'
  return ENVELOPES.find((e) => e.value === envelope)?.label ?? envelope
}

export const HOLDING_KIND_LABEL: Record<HoldingKind, string> = {
  security: 'Titre',
  crypto: 'Crypto',
  fonds_euro: 'Fonds en euros',
  other: 'Autre',
}

export const SECURITY_TYPES: { value: string; label: string }[] = [
  { value: 'etf', label: 'ETF' },
  { value: 'stock', label: 'Action' },
  { value: 'fund', label: 'Fonds' },
  { value: 'bond', label: 'Obligation' },
  { value: 'scpi', label: 'SCPI' },
]

export const PROPERTY_CATEGORY_LABEL: Record<PropertyCategory, string> = {
  primary: 'Résidence principale',
  secondary: 'Résidence secondaire',
  rental: 'Investissement locatif',
}

export const BUILDING_TYPE_LABEL: Record<BuildingType, string> = {
  house: 'Maison',
  apartment: 'Appartement',
  land: 'Terrain',
  parking: 'Parking',
  other: 'Autre',
}

export const GOAL_STATUS: Record<GoalStatus, { label: string; tone: 'pos' | 'neg' | 'brass' | 'accent' }> = {
  completed: { label: 'Atteint', tone: 'pos' },
  funded: { label: 'Financé', tone: 'pos' },
  on_track: { label: 'En bonne voie', tone: 'accent' },
  challenging: { label: 'Ambitieux', tone: 'brass' },
  underfunded: { label: 'Insuffisant', tone: 'brass' },
  overdue: { label: 'Échéance dépassée', tone: 'neg' },
}

/** Couleur de série par catégorie d'allocation : stable d'un écran à l'autre. */
export const CATEGORY_COLOR: Record<AllocationCategory, string> = {
  investment: 'var(--c1)',
  real_estate: 'var(--c2)',
  savings: 'var(--c3)',
  crypto: 'var(--c4)',
  checking: 'var(--c5)',
  other: 'var(--c7)',
}

export const SERIES_COLORS = ['var(--c1)', 'var(--c2)', 'var(--c3)', 'var(--c4)', 'var(--c5)', 'var(--c6)', 'var(--c7)']

export const MEMBER_COLORS = ['#3F51B5', '#B8862B', '#1E8C82', '#8A4B9C', '#C2577A', '#5D7BB8', '#6B8E3A', '#7A8496']
