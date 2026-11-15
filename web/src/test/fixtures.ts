// Données de test entièrement fictives, construites à partir du contrat docs/API.md.
import type { Account, AccountDetail, Goal, Member, Overview, User } from '@/shared/api/types'

export const user: User = { id: 'u-1', email: 'camille@exemple.test', name: 'Camille Martin' }

export const camille: Member = { id: 'm-camille', name: 'Camille', color: '#3F51B5', is_primary: true }
export const sacha: Member = { id: 'm-sacha', name: 'Sacha', color: '#B8862B', is_primary: false }
export const members: Member[] = [camille, sacha]

export function buildAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: 'a-1',
    name: 'Compte joint',
    institution: 'Banque du Canal',
    kind: 'checking',
    envelope: null,
    currency: 'EUR',
    cash_balance: 2400,
    annual_yield_pct: null,
    source: 'manual',
    external_id: null,
    connection_id: null,
    opened_at: null,
    owners: [
      { member_id: camille.id, name: camille.name, share: 0.5 },
      { member_id: sacha.id, name: sacha.name, share: 0.5 },
    ],
    value: 2400,
    owned_value: 2400,
    is_liability: false,
    invested: null,
    gain: null,
    updated_at: '2025-03-01T09:00:00Z',
    ...overrides,
  }
}

export function buildAccountDetail(overrides: Partial<AccountDetail> = {}): AccountDetail {
  return { ...buildAccount(), holdings: [], property: null, loan: null, ...overrides }
}

export function buildOverview(overrides: Partial<Overview> = {}): Overview {
  return {
    gross_assets: 180_000,
    liabilities: 60_000,
    net_worth: 120_000,
    financial_assets: 45_000,
    invested: 30_000,
    gain: 3_000,
    allocation: [
      { category: 'investment', label: 'Investissements', value: 33_000, pct: 18.3 },
      { category: 'savings', label: 'Épargne', value: 12_000, pct: 6.7 },
      { category: 'real_estate', label: 'Immobilier', value: 135_000, pct: 75 },
    ],
    by_institution: [{ institution: 'Banque du Canal', value: 45_000 }],
    by_envelope: [{ envelope: 'pea', value: 33_000 }],
    members: [
      { member_id: camille.id, name: 'Camille', color: camille.color, gross_assets: 100_000, liabilities: 30_000, net_worth: 70_000 },
      { member_id: sacha.id, name: 'Sacha', color: sacha.color, gross_assets: 80_000, liabilities: 30_000, net_worth: 50_000 },
    ],
    top_holdings: [
      { name: 'Fonds Indice Monde', symbol: 'FIM', value: 33_000, gain_pct: 10, account_name: 'PEA Camille' },
    ],
    weighted_expense_ratio: 0.0025,
    ...overrides,
  }
}

export const emptyOverview: Overview = buildOverview({
  gross_assets: 0,
  liabilities: 0,
  net_worth: 0,
  financial_assets: 0,
  invested: 0,
  gain: 0,
  allocation: [],
  by_institution: [],
  by_envelope: [],
  members: [],
  top_holdings: [],
  weighted_expense_ratio: null,
})

export function buildGoal(overrides: Partial<Goal> = {}): Goal {
  return {
    id: 'g-1',
    name: 'Apport appartement',
    icon: null,
    kind: 'project',
    target_amount: 40_000,
    target_date: '2030-06-30',
    monthly_contribution: 400,
    expected_return_pct: 4,
    monthly_expenses: null,
    coverage_months: null,
    account_ids: ['a-1'],
    member_ids: [camille.id],
    progress: {
      current_amount: 10_000,
      target_amount: 40_000,
      pct: 25,
      status: 'on_track',
      required_return_pct: 3.2,
      projected_amount_at_date: 41_500,
      coverage_months: null,
    },
    ...overrides,
  }
}
