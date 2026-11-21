import { request } from '@/shared/api/client'
import { normalizeHistory } from '@/shared/lib/history'
import type {
  AccountDetail,
  Account,
  AccountInput,
  Alert,
  AlertEvaluation,
  AlertRule,
  AlertRuleInput,
  AlertRuleKind,
  AuthResult,
  AuthStatus,
  Benchmark,
  BenchmarkInput,
  BudgetMonth,
  BudgetRule,
  BudgetTarget,
  BudgetTransaction,
  Bundle,
  Connection,
  ConnectionCreated,
  Fx,
  Goal,
  GoalInput,
  History,
  ImportKind,
  ImportProfile,
  ImportProfileInput,
  HistoryCorrectionInput,
  HistoryDeleteResult,
  HistoryRange,
  HistoryRecomputeResult,
  HistoryReview,
  HistoryReviewPoint,
  Holding,
  HoldingInput,
  ImportResult,
  LoanSchedule,
  LoginEvent,
  LoginResult,
  Member,
  MemberInput,
  Operation,
  OperationImportInput,
  OperationInput,
  OperationsBulkResult,
  OperationsSummary,
  Overview,
  PasswordChangeInput,
  PasswordResetInput,
  PerformanceReport,
  PricesRefresh,
  Provider,
  RealEstate,
  SetupInput,
  SyncResult,
  Transaction,
  TransactionPage,
  TotpActivation,
  TotpDisableInput,
  TotpEnrollment,
  TotpStatus,
  TransactionPatch,
  TransactionsBulkInput,
  TransactionsBulkResult,
  User,
} from '@/shared/api/types'

/** `member_id` absent = vue foyer. */
type View = { member_id: string | null }

export const api = {
  // Auth
  authStatus: () => request<AuthStatus>('/api/auth/status'),
  setup: (body: SetupInput) =>
    request<AuthResult>('/api/auth/setup', { method: 'POST', body }),
  login: (body: { email: string; password: string }) =>
    request<LoginResult>('/api/auth/login', { method: 'POST', body }),
  loginTotp: (body: { challenge: string; code: string }) =>
    request<AuthResult>('/api/auth/login/totp', { method: 'POST', body }),
  me: () => request<User>('/api/auth/me'),
  changePassword: (body: PasswordChangeInput) =>
    request<AuthResult>('/api/auth/password', { method: 'PUT', body }),
  resetPassword: (body: PasswordResetInput) =>
    request<AuthResult>('/api/auth/reset', { method: 'POST', body }),
  refreshSession: () => request<AuthResult>('/api/auth/refresh', { method: 'POST' }),
  loginHistory: (limit?: number) => request<LoginEvent[]>('/api/auth/logins', { query: { limit } }),
  totpStatus: () => request<TotpStatus>('/api/auth/totp'),
  totpSetup: () => request<TotpEnrollment>('/api/auth/totp/setup', { method: 'POST' }),
  totpEnable: (code: string) =>
    request<TotpActivation>('/api/auth/totp/enable', { method: 'POST', body: { code } }),
  totpDisable: (body: TotpDisableInput) =>
    request<AuthResult>('/api/auth/totp/disable', { method: 'POST', body }),
  totpRecoveryCodes: (code: string) =>
    request<{ recovery_codes: string[] }>('/api/auth/totp/recovery-codes', { method: 'POST', body: { code } }),

  // Membres
  members: () => request<Member[]>('/api/members'),
  createMember: (body: MemberInput) => request<Member>('/api/members', { method: 'POST', body }),
  updateMember: (id: string, body: Partial<MemberInput>) =>
    request<Member>(`/api/members/${id}`, { method: 'PUT', body }),
  deleteMember: (id: string) => request<void>(`/api/members/${id}`, { method: 'DELETE' }),

  // Comptes
  accounts: (view: View) => request<Account[]>('/api/accounts', { query: view }),
  account: (id: string, view: View) => request<AccountDetail>(`/api/accounts/${id}`, { query: view }),
  createAccount: (body: AccountInput) => request<AccountDetail>('/api/accounts', { method: 'POST', body }),
  updateAccount: (id: string, body: Partial<AccountInput>) =>
    request<AccountDetail>(`/api/accounts/${id}`, { method: 'PUT', body }),
  deleteAccount: (id: string) => request<void>(`/api/accounts/${id}`, { method: 'DELETE' }),

  // Positions
  createHolding: (accountId: string, body: HoldingInput) =>
    request<Holding>(`/api/accounts/${accountId}/holdings`, { method: 'POST', body }),
  bulkHoldings: (accountId: string, body: { holdings: HoldingInput[]; replace: boolean }) =>
    request<Holding[]>(`/api/accounts/${accountId}/holdings/bulk`, { method: 'POST', body }),
  updateHolding: (id: string, body: Partial<HoldingInput>) =>
    request<Holding>(`/api/holdings/${id}`, { method: 'PUT', body }),
  deleteHolding: (id: string) => request<void>(`/api/holdings/${id}`, { method: 'DELETE' }),
  refreshPrices: () => request<PricesRefresh>('/api/prices/refresh', { method: 'POST' }),
  fx: () => request<Fx>('/api/fx'),

  // Transactions
  transactions: (accountId: string, page: { limit: number; offset: number }) =>
    request<TransactionPage>(`/api/accounts/${accountId}/transactions`, { query: page }),
  bulkTransactions: (accountId: string, body: TransactionsBulkInput) =>
    request<TransactionsBulkResult>(`/api/accounts/${accountId}/transactions/bulk`, { method: 'POST', body }),
  updateTransaction: (id: string, body: TransactionPatch) =>
    request<Transaction>(`/api/transactions/${id}`, { method: 'PUT', body }),
  deleteTransaction: (id: string) => request<void>(`/api/transactions/${id}`, { method: 'DELETE' }),

  // Budget
  budget: (month: string, view: View) => request<BudgetMonth>('/api/budget', { query: { month, ...view } }),
  budgetTransactions: (month: string, view: View) =>
    request<BudgetTransaction[]>('/api/budget/transactions', { query: { month, ...view } }),
  budgetCategories: () => request<string[]>('/api/budget/categories'),
  budgetRules: () => request<BudgetRule[]>('/api/budget/rules'),
  deleteBudgetRule: (id: string) => request<void>(`/api/budget/rules/${id}`, { method: 'DELETE' }),
  budgetTargets: () => request<BudgetTarget[]>('/api/budget/targets'),
  setBudgetTarget: (body: { category: string; monthly_amount: number | null }) =>
    request<BudgetTarget[]>('/api/budget/targets', { method: 'PUT', body }),

  // Journal d'opérations (comptes d'investissement)
  operations: (accountId: string) => request<Operation[]>(`/api/accounts/${accountId}/operations`),
  operationsSummary: (accountId: string) =>
    request<OperationsSummary>(`/api/accounts/${accountId}/operations/summary`),
  createOperation: (accountId: string, body: OperationInput) =>
    request<Operation>(`/api/accounts/${accountId}/operations`, { method: 'POST', body }),
  updateOperation: (id: string, body: OperationInput) =>
    request<Operation>(`/api/operations/${id}`, { method: 'PUT', body }),
  deleteOperation: (id: string) => request<void>(`/api/operations/${id}`, { method: 'DELETE' }),
  bulkOperations: (accountId: string, body: { operations: OperationImportInput[] }) =>
    request<OperationsBulkResult>(`/api/accounts/${accountId}/operations/bulk`, { method: 'POST', body }),

  // Préférences d'import CSV
  importProfiles: (accountId: string) => request<ImportProfile[]>(`/api/accounts/${accountId}/import-profiles`),
  saveImportProfile: (accountId: string, kind: ImportKind, body: ImportProfileInput) =>
    request<ImportProfile>(`/api/accounts/${accountId}/import-profiles/${kind}`, { method: 'PUT', body }),

  // Immobilier et prêts
  realEstate: (view: View) => request<RealEstate>('/api/real-estate', { query: view }),
  loanSchedule: (accountId: string, view: View) =>
    request<LoanSchedule>(`/api/loans/${accountId}/schedule`, { query: view }),

  // Vue d'ensemble
  overview: (view: View) => request<Overview>('/api/overview', { query: view }),
  history: (range: HistoryRange, view: View): Promise<History> =>
    request<History>('/api/history', { query: { range, ...view } }).then(normalizeHistory),

  // Performance et indice de référence
  performance: (period: { from?: string; to?: string }, view: View) =>
    request<PerformanceReport>('/api/performance', { query: { ...period, ...view } }),
  benchmark: () => request<Benchmark | null>('/api/performance/benchmark'),
  setBenchmark: (body: BenchmarkInput) =>
    request<Benchmark>('/api/performance/benchmark', { method: 'PUT', body }),
  deleteBenchmark: () => request<void>('/api/performance/benchmark', { method: 'DELETE' }),

  // Nettoyage de l'historique
  historyReview: (view: View) => request<HistoryReview>('/api/history/points', { query: view }),
  correctHistoryPoint: (date: string, view: View, body: HistoryCorrectionInput) =>
    request<HistoryReviewPoint>(`/api/history/points/${date}`, { method: 'PUT', query: view, body }),
  deleteHistoryRange: (range: { from: string; to: string }) =>
    request<HistoryDeleteResult>('/api/history/points', { method: 'DELETE', query: range }),
  recomputeHistory: () => request<HistoryRecomputeResult>('/api/history/recompute', { method: 'POST' }),
  resetHistory: (confirmation: string) =>
    request<HistoryDeleteResult>('/api/history/reset', { method: 'POST', body: { confirmation } }),

  // Objectifs
  goals: (view: View) => request<Goal[]>('/api/goals', { query: view }),
  createGoal: (body: GoalInput) => request<Goal>('/api/goals', { method: 'POST', body }),
  updateGoal: (id: string, body: Partial<GoalInput>) => request<Goal>(`/api/goals/${id}`, { method: 'PUT', body }),
  deleteGoal: (id: string) => request<void>(`/api/goals/${id}`, { method: 'DELETE' }),

  // Connexions bancaires
  providers: () => request<Provider[]>('/api/connections/providers'),
  connections: () => request<Connection[]>('/api/connections'),
  createConnection: (provider: string) =>
    request<ConnectionCreated>('/api/connections', { method: 'POST', body: { provider } }),
  syncConnection: (id: string) => request<SyncResult>(`/api/connections/${id}/sync`, { method: 'POST' }),
  deleteConnection: (id: string) => request<void>(`/api/connections/${id}`, { method: 'DELETE' }),

  // Alertes et rappels
  alerts: (archived = false) =>
    request<Alert[]>('/api/alerts', { query: archived ? { archived: 'true' } : undefined }),
  evaluateAlerts: () => request<AlertEvaluation>('/api/alerts/evaluate', { method: 'POST' }),
  updateAlert: (id: string, body: { read?: boolean; archived?: boolean }) =>
    request<Alert>(`/api/alerts/${id}`, { method: 'PUT', body }),
  readAllAlerts: () => request<{ updated: number }>('/api/alerts/read-all', { method: 'POST' }),
  alertRules: () => request<AlertRule[]>('/api/alerts/rules'),
  updateAlertRule: (kind: AlertRuleKind, body: AlertRuleInput) =>
    request<AlertRule>(`/api/alerts/rules/${kind}`, { method: 'PUT', body }),

  // Import / export complet
  exportBundle: () => request<Bundle>('/api/export'),
  importBundle: (bundle: Bundle) => request<ImportResult>('/api/import', { method: 'POST', body: bundle }),
}
