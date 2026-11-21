// Types du contrat docs/API.md. Toute divergence front ↔ back se corrige ici.

export type User = { id: string; email: string; name: string }
/** `setup_token_required` et `recovery_enabled` peuvent manquer sur une API plus ancienne : absent = false. */
export type AuthStatus = { setup_required: boolean; setup_token_required?: boolean; recovery_enabled?: boolean }
export type SetupInput = { email: string; password: string; name: string; setup_token?: string }
export type AuthResult = { token: string; user: User }
export type PasswordChangeInput = { current_password: string; new_password: string }
export type PasswordResetInput = { email: string; recovery_token: string; new_password: string }
/** Tentative de connexion (`login`) ou de réinitialisation (`reset`) ; `at` en RFC 3339. */
export type LoginEvent = {
  at: string
  action: 'login' | 'reset'
  success: boolean
  ip: string | null
  user_agent: string | null
}

export type Member = { id: string; name: string; color: string; is_primary: boolean }
export type MemberInput = { name: string; color?: string }

export type AccountKind =
  | 'checking'
  | 'savings'
  | 'investment'
  | 'crypto'
  | 'real_estate'
  | 'loan'
  | 'other'

/** Valeurs connues ; le champ reste libre côté API. */
export type Envelope = 'pea' | 'pea_pme' | 'cto' | 'assurance_vie' | 'per' | 'livret' | 'pee' | (string & {}) | null

export type AccountOwner = { member_id: string; name: string; share: number }
export type OwnerInput = { member_id: string; share: number }

export type Account = {
  id: string
  name: string
  institution: string | null
  kind: AccountKind
  envelope: Envelope
  currency: string
  cash_balance: number
  annual_yield_pct: number | null
  source: 'manual' | 'csv' | 'aggregator' | 'import'
  external_id: string | null
  connection_id: string | null
  opened_at: string | null
  owners: AccountOwner[]
  value: number
  owned_value: number
  is_liability: boolean
  invested: number | null
  gain: number | null
  updated_at: string
}

export type HoldingKind = 'security' | 'crypto' | 'fonds_euro' | 'other'

export type Holding = {
  id: string
  account_id: string
  kind: HoldingKind
  name: string
  isin: string | null
  symbol: string | null
  security_type: string | null
  quantity: number
  buy_price: number | null
  current_price: number
  currency: string
  expense_ratio: number | null
  price_source: 'manual' | 'auto'
  price_updated_at: string | null
  /** Cours automatique que le fournisseur ne met plus à jour (seuil fixé par l'API). */
  price_stale: boolean
  value: number
  invested: number | null
  gain: number | null
  gain_pct: number | null
}

/** Non détaillé dans le contrat : champs saisissables de Holding. */
export type HoldingInput = {
  kind: HoldingKind
  name: string
  isin?: string | null
  symbol?: string | null
  security_type?: string | null
  quantity: number
  buy_price?: number | null
  current_price: number
  currency?: string
  expense_ratio?: number | null
}

export type PropertyCategory = 'primary' | 'secondary' | 'rental'
export type BuildingType = 'house' | 'apartment' | 'land' | 'parking' | 'other'

export type Property = {
  id: string
  account_id: string
  address: string | null
  category: PropertyCategory
  building_type: BuildingType
  surface_m2: number | null
  purchase_price: number
  purchase_fees: number
  purchased_at: string | null
  current_value: number
  monthly_rent: number
  monthly_charges: number
  yearly_property_tax: number
}

/** Non détaillé dans le contrat : Property sans id ni account_id. */
export type PropertyInput = Omit<Property, 'id' | 'account_id'>

export type Loan = {
  id: string
  account_id: string
  property_account_id: string | null
  principal: number
  interest_rate_pct: number
  insurance_rate_pct: number
  duration_months: number
  start_date: string
  monthly_payment: number
  monthly_insurance: number
  remaining_capital: number
  end_date: string
  total_interest: number
  total_insurance: number
}

/** Non détaillé dans le contrat : champs non calculés de Loan. */
export type LoanInput = {
  property_account_id: string | null
  principal: number
  interest_rate_pct: number
  insurance_rate_pct: number
  duration_months: number
  start_date: string
}

export type AccountDetail = Account & {
  holdings: Holding[]
  property: Property | null
  loan: Loan | null
}

export type AccountInput = {
  name: string
  institution?: string | null
  kind: AccountKind
  envelope?: Envelope
  currency?: string
  cash_balance?: number
  annual_yield_pct?: number | null
  opened_at?: string | null
  owners: OwnerInput[]
  property?: PropertyInput
  loan?: LoanInput
}

export type PricesRefresh = {
  updated: number
  failed: { holding_id: string; name: string; reason: string }[]
}

export type Fx = { base: 'EUR'; rates: Record<string, number>; updated_at: string }

export type Transaction = {
  id: string
  account_id: string
  date: string
  label: string
  amount: number
  /** Catégorie reçue à l'import. */
  category: string | null
  /** Choix manuels du budget ; `null` = automatique. */
  manual_category: string | null
  internal_transfer: boolean | null
}
export type TransactionPatch = { category?: string | null; internal_transfer?: boolean | null }
export type TransactionInput = { date: string; label: string; amount: number; category?: string | null }
export type TransactionPage = { items: Transaction[]; total: number }
export type TransactionsBulkInput = { transactions: TransactionInput[]; new_cash_balance?: number }
export type TransactionsBulkResult = { inserted: number; skipped: number }

export type OperationKind = 'buy' | 'sell' | 'dividend' | 'fee' | 'deposit' | 'withdrawal'
/** Opération du journal d'un compte d'investissement. */
export type Operation = {
  id: string
  account_id: string
  kind: OperationKind
  date: string
  holding_id: string | null
  holding_name: string | null
  quantity: number | null
  unit_price: number | null
  /** Achat/vente : quantité × cours, hors frais. */
  amount: number
  fees: number
  currency: string
  note: string | null
  /** Vente uniquement : plus-value réalisée nette des frais. */
  realized_gain: number | null
}
export type OperationInput = {
  kind: OperationKind
  date: string
  holding_id?: string
  quantity?: number
  unit_price?: number
  fees?: number
  amount?: number
  note?: string
}
export type OperationYear = {
  year: number
  realized_gain: number
  dividends: number
  fees: number
  deposits: number
  withdrawals: number
}
export type OperationsSummary = { currency: 'EUR'; years: OperationYear[] }

/** Mesures de performance ; une fraction (0.05 = +5 %), null = indéfini. */
export type Performance = {
  from: string
  to: string
  start_value: number
  end_value: number
  net_flows: number
  gain: number
  twr: number | null
  twr_annualized: number | null
  mwr: number | null
  mwr_annualized: number | null
  years: { year: number; from: string; to: string; twr: number | null }[]
  /** Courbe base 100 ; `benchmark` : indice ramené au même niveau, null hors fenêtre de comparaison. */
  points: { date: string; value: number; benchmark: number | null }[]
}
export type BenchmarkKind = 'security' | 'crypto'
export type Benchmark = {
  name: string
  symbol: string | null
  isin: string | null
  kind: BenchmarkKind
  currency: string
  prices: number
  first_date: string | null
  last_date: string | null
  last_error: string | null
}
export type BenchmarkInput = { name: string; symbol?: string; isin?: string; kind: BenchmarkKind; currency: string }
export type BenchmarkComparison = { from: string; to: string; benchmark_return: number; portfolio_return: number }
export type PerformanceReport = {
  portfolio: Performance | null
  accounts: { account_id: string; name: string; kind: AccountKind; performance: Performance | null }[]
  benchmark: (Benchmark & { comparison: BenchmarkComparison | null; message: string | null }) | null
}

export type RealEstateLoan = Loan & { account_id: string; name: string }
export type RealEstateItem = {
  account: Account
  property: Property
  loans: RealEstateLoan[]
  equity: number
  gross_yield_pct: number | null
  net_yield_pct: number | null
  latent_gain: number
}
export type RealEstate = {
  properties: RealEstateItem[]
  totals: {
    value: number
    remaining_debt: number
    equity: number
    monthly_rent: number
    monthly_payments: number
  }
}

export type ScheduleRow = {
  month: number
  date: string
  payment: number
  interest: number
  principal: number
  insurance: number
  remaining: number
}
export type LoanSchedule = { loan: Loan; rows: ScheduleRow[] }

export type AllocationCategory = 'checking' | 'savings' | 'investment' | 'crypto' | 'real_estate' | 'other'

export type Overview = {
  gross_assets: number
  liabilities: number
  net_worth: number
  financial_assets: number
  invested: number
  gain: number
  allocation: { category: AllocationCategory; label: string; value: number; pct: number }[]
  by_institution: { institution: string; value: number }[]
  by_envelope: { envelope: string; value: number }[]
  members: {
    member_id: string
    name: string
    color: string
    gross_assets: number
    liabilities: number
    net_worth: number
  }[]
  top_holdings: {
    name: string
    symbol: string | null
    value: number
    gain_pct: number | null
    account_name: string
  }[]
  weighted_expense_ratio: number | null
}

export type HistoryRange = '1m' | '3m' | '1y' | 'all'
/** Valeur détenue d'un compte dans un instantané. `kind = loan` : dette. */
export type HistoryAccount = {
  account_id: string
  name: string
  kind: AccountKind
  value: number
  gain: number | null
}
export type HistoryPoint = {
  date: string
  gross_assets: number
  liabilities: number
  net_worth: number
  /** null : instantané enregistré avant l'introduction du détail par compte. */
  accounts: HistoryAccount[] | null
}
export type History = { points: HistoryPoint[] }

/** Point de l'historique tel que le voit l'écran de nettoyage. */
export type HistoryReviewPoint = {
  date: string
  gross_assets: number
  liabilities: number
  net_worth: number
  /** false : pas de détail par compte (instantané ancien ou corrigé). */
  detailed: boolean
  corrected: boolean
  /** Variation depuis le point précédent, fraction ; null pour le premier point ou une base nulle. */
  change: number | null
  suspicious: boolean
}
export type HistoryReview = { threshold: number; points: HistoryReviewPoint[] }
export type HistoryCorrectionInput = { gross_assets: number; liabilities: number }
export type HistoryDeleteResult = { deleted_days: number }
export type HistoryRecomputeResult = { snapshot_date: string }

export type GoalKind = 'safety_net' | 'project'
export type GoalStatus = 'completed' | 'on_track' | 'challenging' | 'overdue' | 'underfunded' | 'funded'

export type Goal = {
  id: string
  name: string
  icon: string | null
  kind: GoalKind
  target_amount: number | null
  target_date: string | null
  monthly_contribution: number | null
  expected_return_pct: number | null
  monthly_expenses: number | null
  coverage_months: number | null
  account_ids: string[]
  member_ids: string[]
  progress: {
    current_amount: number
    target_amount: number
    pct: number
    status: GoalStatus
    required_return_pct: number | null
    projected_amount_at_date: number | null
    coverage_months: number | null
  }
}

/** Non détaillé dans le contrat : Goal sans id ni progress. */
export type GoalInput = Omit<Goal, 'id' | 'progress'>

export type Provider = { id: string; name: string; configured: boolean }
export type Connection = {
  id: string
  provider: string
  institution: string | null
  status: 'active' | 'error' | 'expired'
  last_sync_at: string | null
  error: string | null
}
export type ConnectionCreated = { id: string; redirect_url: string }
export type SyncResult = { accounts_updated: number; transactions_inserted: number }

/** Le lot d'export est renvoyé tel quel à l'import : on ne le manipule pas côté front. */
export type Bundle = {
  version: 1
  members: unknown[]
  accounts: unknown[]
  goals: unknown[]
}
export type ImportResult = { members: number; accounts: number; holdings: number; goals: number }

// Budget
export type BudgetCategorySource = 'manual' | 'rule' | 'import' | 'default'
export type BudgetFlows = {
  /** `YYYY-MM` */
  month: string
  income: number
  expenses: number
  savings: number
  savings_rate: number | null
}
export type BudgetCategory = { category: string | null; amount: number; target: number | null }
export type RecurringCharge = {
  label: string
  category: string | null
  average_amount: number
  occurrences: number
  last_date: string
  next_date: string
}
export type BudgetMonth = BudgetFlows & {
  categories: BudgetCategory[]
  trend: BudgetFlows[]
  recurring: RecurringCharge[]
  internal_transfers: number
}
export type BudgetTransaction = {
  id: string
  account_id: string
  account_name: string
  date: string
  label: string
  amount: number
  amount_eur: number
  category: string | null
  category_source: BudgetCategorySource | null
  internal_transfer: boolean
  internal_transfer_manual: boolean
}
export type BudgetRule = { id: string; pattern: string; category: string }
export type BudgetTarget = { category: string; monthly_amount: number }
