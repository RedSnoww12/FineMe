import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { HoldingInput, OperationInput, PricesRefresh, TransactionInput } from '@/shared/api/types'

export function useOperations(accountId: string) {
  return useQuery({ queryKey: ['operations', accountId], queryFn: () => api.operations(accountId) })
}

export function useOperationsSummary(accountId: string) {
  return useQuery({ queryKey: ['operations-summary', accountId], queryFn: () => api.operationsSummary(accountId) })
}

export function useCreateOperation(accountId: string, onSuccess: () => void) {
  return useApiMutation((input: OperationInput) => api.createOperation(accountId, input), onSuccess)
}

export function useDeleteOperation(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteOperation(id), onSuccess)
}

export function useTransactions(accountId: string, limit: number, offset: number) {
  return useQuery({
    queryKey: ['transactions', accountId, limit, offset],
    queryFn: () => api.transactions(accountId, { limit, offset }),
    placeholderData: keepPreviousData,
  })
}

export function useDeleteTransaction(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteTransaction(id), onSuccess)
}

/** Crée la position, ou la met à jour quand `holdingId` est fourni. */
export function useSaveHolding(accountId: string, holdingId: string | undefined, onSuccess: () => void) {
  return useApiMutation(
    (input: HoldingInput) => (holdingId ? api.updateHolding(holdingId, input) : api.createHolding(accountId, input)),
    onSuccess,
  )
}

export function useDeleteHolding(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteHolding(id), onSuccess)
}

export function useRefreshPrices(onSuccess: (result: PricesRefresh) => void) {
  return useApiMutation(() => api.refreshPrices(), onSuccess)
}

export function useImportTransactions(
  accountId: string,
  onSuccess: (result: { inserted: number; skipped: number }) => void,
) {
  return useApiMutation(
    (input: { transactions: TransactionInput[]; newCashBalance: number | null }) =>
      api.bulkTransactions(accountId, {
        transactions: input.transactions,
        ...(input.newCashBalance !== null ? { new_cash_balance: input.newCashBalance } : {}),
      }),
    onSuccess,
  )
}

export function useImportHoldings(accountId: string, onSuccess: (imported: number) => void) {
  return useApiMutation(
    (input: { holdings: HoldingInput[]; replace: boolean }) => api.bulkHoldings(accountId, input),
    (holdings) => onSuccess(holdings.length),
  )
}
