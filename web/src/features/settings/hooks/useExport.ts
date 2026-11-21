import { useMutation, useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useMembers } from '@/shared/api/queries'
import type { Account, Transaction } from '@/shared/api/types'
import { accountsTable, holdingsTable, transactionsTable, type Table } from '../lib/tabular'

/** Taille de page maximale acceptée par l'API pour les transactions. */
const TRANSACTIONS_PAGE = 1000

const WITHOUT_HOLDINGS = new Set<Account['kind']>(['real_estate', 'loan'])

async function allTransactions(accountId: string): Promise<Transaction[]> {
  const items: Transaction[] = []
  for (;;) {
    const page = await api.transactions(accountId, { limit: TRANSACTIONS_PAGE, offset: items.length })
    items.push(...page.items)
    if (page.items.length === 0 || items.length >= page.total) return items
  }
}

export type ExportTables = { accounts: Table; holdings: Table; transactions: Table }

/** Lit comptes, positions et transactions de la vue choisie, puis les met en tableaux. */
export async function fetchExportTables(memberId: string | null): Promise<ExportTables> {
  const view = { member_id: memberId }
  const accounts = await api.accounts(view)
  const [details, transactions] = await Promise.all([
    Promise.all(accounts.filter((a) => !WITHOUT_HOLDINGS.has(a.kind)).map((a) => api.account(a.id, view))),
    Promise.all(accounts.map(async (account) => ({ account, transactions: await allTransactions(account.id) }))),
  ])
  return {
    accounts: accountsTable(accounts, memberId),
    holdings: holdingsTable(details, memberId),
    transactions: transactionsTable(transactions, memberId),
  }
}

export function useExportTables() {
  return useMutation({ mutationFn: fetchExportTables })
}

/** Options du sélecteur de périmètre (`''` = foyer) et libellé d'un périmètre. */
export function useScopeOptions() {
  const members = useMembers()
  const options = [{ value: '', label: 'Foyer' }, ...(members.data ?? []).map((m) => ({ value: m.id, label: m.name }))]
  const labelOf = (memberId: string | null) => options.find((o) => o.value === (memberId ?? ''))?.label ?? 'Foyer'
  return { options, labelOf }
}

/** Tout ce que le rapport imprimable affiche, pour la vue choisie. */
export function useReportData(memberId: string | null) {
  const view = { member_id: memberId }
  const overview = useQuery({ queryKey: ['overview', memberId], queryFn: () => api.overview(view) })
  const accounts = useQuery({ queryKey: ['accounts', memberId], queryFn: () => api.accounts(view) })
  const goals = useQuery({ queryKey: ['goals', memberId], queryFn: () => api.goals(view) })
  const history = useQuery({ queryKey: ['history', '1y', memberId], queryFn: () => api.history('1y', view) })
  return { overview, accounts, goals, history }
}
