// Données de test entièrement fictives, construites à partir du contrat docs/API.md.
import type {
  Account,
  AccountDetail,
  Goal,
  HistoryAccount,
  HistoryPoint,
  HistoryReviewPoint,
  Holding,
  Member,
  Overview,
  RealEstate,
  RealEstateItem,
  RealEstateLoan,
  ScheduleRow,
  User,
} from '@/shared/api/types'

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
    management_fee_pct: null,
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

export function buildHolding(overrides: Partial<Holding> = {}): Holding {
  return {
    id: 'h-1',
    account_id: 'a-1',
    kind: 'security',
    name: 'Fonds Indice Monde',
    isin: 'FR0000000001',
    symbol: 'FIM.PA',
    security_type: 'etf',
    quantity: 100,
    buy_price: 300,
    current_price: 330,
    currency: 'EUR',
    expense_ratio: 0.002,
    price_source: 'auto',
    price_updated_at: '2025-03-03T06:20:00Z',
    price_stale: false,
    value: 33_000,
    invested: 30_000,
    gain: 3_000,
    gain_pct: 10,
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

/** Ligne de détail d'un instantané. */
export function buildHistoryAccount(overrides: Partial<HistoryAccount> = {}): HistoryAccount {
  return { account_id: 'a-1', name: 'Compte joint', kind: 'checking', value: 2400, gain: null, ...overrides }
}

/** Instantané détaillé : les totaux sont déduits des comptes, sauf s'ils sont donnés. */
export function buildHistoryPoint(date: string, accounts: HistoryAccount[] | null, overrides: Partial<HistoryPoint> = {}): HistoryPoint {
  const lines = accounts ?? []
  const gross = lines.filter((a) => a.kind !== 'loan').reduce((sum, a) => sum + a.value, 0)
  const debts = lines.filter((a) => a.kind === 'loan').reduce((sum, a) => sum + a.value, 0)
  return { date, gross_assets: gross, liabilities: debts, net_worth: gross - debts, accounts, ...overrides }
}

/** Point de l'écran de nettoyage : sans dette, ni suspect ni corrigé par défaut. */
export function buildHistoryReviewPoint(date: string, overrides: Partial<HistoryReviewPoint> = {}): HistoryReviewPoint {
  const netWorth = overrides.net_worth ?? 100_000
  return {
    date,
    gross_assets: netWorth,
    liabilities: 0,
    net_worth: netWorth,
    detailed: true,
    corrected: false,
    change: null,
    suspicious: false,
    ...overrides,
  }
}

export function buildRealEstateLoan(overrides: Partial<RealEstateLoan> = {}): RealEstateLoan {
  return {
    id: 'l-1',
    account_id: 'a-pret',
    name: 'Prêt maison',
    property_account_id: 'a-maison',
    principal: 1200,
    interest_rate_pct: 0,
    insurance_rate_pct: 0,
    duration_months: 12,
    start_date: '2026-01-01',
    monthly_payment: 100,
    monthly_insurance: 0,
    remaining_capital: 600,
    end_date: '2026-12-01',
    total_interest: 0,
    total_insurance: 0,
    ...overrides,
  }
}

/** Tableau d'amortissement sans intérêts : `principal / months` remboursés chaque mois. */
export function buildScheduleRows(principal: number, months: number, firstDate = '2026-02-01'): ScheduleRow[] {
  const [year, month] = firstDate.split('-').map(Number)
  const step = principal / months
  return Array.from({ length: months }, (_, i) => {
    const index = year * 12 + (month - 1) + i
    const date = `${Math.floor(index / 12)}-${String((index % 12) + 1).padStart(2, '0')}-01`
    return { month: i + 1, date, payment: step, interest: 0, principal: step, insurance: 0, remaining: principal - step * (i + 1) }
  })
}

export function buildRealEstateItem(overrides: Partial<RealEstateItem> = {}): RealEstateItem {
  return {
    account: buildAccount({ id: 'a-maison', name: 'Maison', kind: 'real_estate', value: 2000, owned_value: 2000, cash_balance: 0 }),
    property: {
      id: 'p-1',
      account_id: 'a-maison',
      address: null,
      category: 'primary',
      building_type: 'house',
      surface_m2: null,
      purchase_price: 1800,
      purchase_fees: 100,
      purchased_at: null,
      current_value: 2000,
      monthly_rent: 0,
      monthly_charges: 0,
      yearly_property_tax: 0,
    },
    loans: [buildRealEstateLoan()],
    equity: 1400,
    gross_yield_pct: null,
    net_yield_pct: null,
    latent_gain: 100,
    ...overrides,
  }
}

export function buildRealEstate(items: RealEstateItem[] = [buildRealEstateItem()]): RealEstate {
  const value = items.reduce((sum, item) => sum + item.account.owned_value, 0)
  const equity = items.reduce((sum, item) => sum + item.equity, 0)
  return {
    properties: items,
    totals: { value, remaining_debt: value - equity, equity, monthly_rent: 0, monthly_payments: 100 },
  }
}
