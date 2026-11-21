import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type {
  HoldingInput,
  ImportKind,
  ImportProfileInput,
  OperationImportInput,
  OperationInput,
  OperationsBulkResult,
  PricesRefresh,
  TransactionInput,
} from '@/shared/api/types'

export function useOperations(accountId: string) {
  return useQuery({ queryKey: ['operations', accountId], queryFn: () => api.operations(accountId) })
}

export function useOperationsSummary(accountId: string) {
  return useQuery({ queryKey: ['operations-summary', accountId], queryFn: () => api.operationsSummary(accountId) })
}

/**
 * Crée l'opération, ou la remplace quand `operationId` est fourni. Le succès invalide toutes les requêtes :
 * position (quantité, PRU), journal, synthèse annuelle et patrimoine se rechargent.
 */
export function useSaveOperation(accountId: string, operationId: string | undefined, onSuccess: () => void) {
  return useApiMutation(
    (input: OperationInput) =>
      operationId ? api.updateOperation(operationId, input) : api.createOperation(accountId, input),
    onSuccess,
  )
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

export function useImportOperations(accountId: string, onSuccess: (result: OperationsBulkResult) => void) {
  return useApiMutation(
    (operations: OperationImportInput[]) => api.bulkOperations(accountId, { operations }),
    onSuccess,
  )
}

/** Correspondances de colonnes mémorisées pour ce compte (une par type de contenu). */
export function useImportProfiles(accountId: string) {
  return useQuery({ queryKey: ['import-profiles', accountId], queryFn: () => api.importProfiles(accountId) })
}

/** Mémorise la correspondance utilisée, sans rafraîchir le reste de l'application. */
export function useSaveImportProfile(accountId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ kind, ...body }: ImportProfileInput & { kind: ImportKind }) =>
      api.saveImportProfile(accountId, kind, body),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['import-profiles', accountId] }),
  })
}
