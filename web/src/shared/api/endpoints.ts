import { request } from '@/shared/api/client'
import { normalizeHistory } from '@/shared/lib/history'
import type {
  AccountDetail,
  Account,
  AccountInput,
  AuthResult,
  AuthStatus,
  Bundle,
  Connection,
  ConnectionCreated,
  Fx,
  Goal,
  GoalInput,
  History,
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
  Member,
  MemberInput,
  Overview,
  PasswordChangeInput,
  PasswordResetInput,
  PricesRefresh,
  Provider,
  RealEstate,
  SetupInput,
  SyncResult,
  TransactionPage,
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
    request<AuthResult>('/api/auth/login', { method: 'POST', body }),
  me: () => request<User>('/api/auth/me'),
  changePassword: (body: PasswordChangeInput) =>
    request<AuthResult>('/api/auth/password', { method: 'PUT', body }),
  resetPassword: (body: PasswordResetInput) =>
    request<AuthResult>('/api/auth/reset', { method: 'POST', body }),
  refreshSession: () => request<AuthResult>('/api/auth/refresh', { method: 'POST' }),
  loginHistory: (limit?: number) => request<LoginEvent[]>('/api/auth/logins', { query: { limit } }),

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
  deleteTransaction: (id: string) => request<void>(`/api/transactions/${id}`, { method: 'DELETE' }),

  // Immobilier et prêts
  realEstate: (view: View) => request<RealEstate>('/api/real-estate', { query: view }),
  loanSchedule: (accountId: string, view: View) =>
    request<LoanSchedule>(`/api/loans/${accountId}/schedule`, { query: view }),

  // Vue d'ensemble
  overview: (view: View) => request<Overview>('/api/overview', { query: view }),
  history: (range: HistoryRange, view: View): Promise<History> =>
    request<History>('/api/history', { query: { range, ...view } }).then(normalizeHistory),

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

  // Import / export complet
  exportBundle: () => request<Bundle>('/api/export'),
  importBundle: (bundle: Bundle) => request<ImportResult>('/api/import', { method: 'POST', body: bundle }),
}
