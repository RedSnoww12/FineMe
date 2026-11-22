import { keepPreviousData, useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useAccounts, useApiMutation } from '@/shared/api/queries'
import { useView } from '@/shared/state/view'
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

/** Détail (positions comprises) de tous les comptes pouvant porter des positions. Clé partagée avec la fiche d'un compte. */
export function useAllAccountDetails() {
  const { memberId } = useView()
  const accounts = useAccounts()
  const ids = (accounts.data ?? []).filter((a) => a.kind !== 'real_estate' && a.kind !== 'loan').map((a) => a.id)
  const details = useQueries({
    queries: ids.map((id) => ({
      queryKey: ['account', id, memberId],
      queryFn: () => api.account(id, { member_id: memberId }),
    })),
    combine: (results) => ({
      data: results.every((r) => r.data) ? results.flatMap((r) => (r.data ? [r.data] : [])) : undefined,
      isPending: results.some((r) => r.isPending),
      error: results.find((r) => r.error)?.error ?? null,
      refetch: () => results.forEach((r) => void r.refetch()),
    }),
  })
  return {
    data: accounts.data ? details.data : undefined,
    isPending: accounts.isPending || details.isPending,
    error: accounts.error ?? details.error,
    refetch: () => {
      void accounts.refetch()
      details.refetch()
    },
  }
}

/** Journaux d'opérations des comptes donnés, réunis. Clé partagée avec le journal d'un compte. */
export function useOperationsOf(accountIds: readonly string[]) {
  return useQueries({
    queries: accountIds.map((id) => ({ queryKey: ['operations', id], queryFn: () => api.operations(id) })),
    combine: (results) => ({
      data: results.every((r) => r.data) ? results.flatMap((r) => r.data ?? []) : undefined,
      isPending: results.some((r) => r.isPending),
      error: results.find((r) => r.error)?.error ?? null,
    }),
  })
}

export function useHoldingPrices(holdingId: string | undefined) {
  return useQuery({
    queryKey: ['holding-prices', holdingId],
    queryFn: () => api.holdingPrices(holdingId as string, 'all'),
    enabled: Boolean(holdingId),
  })
}
