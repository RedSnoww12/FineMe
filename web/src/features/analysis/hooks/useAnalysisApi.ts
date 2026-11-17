import { keepPreviousData, useQueries, useQuery, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { HistoryRange } from '@/shared/api/types'
import { useView } from '@/shared/state/view'

/** Réunit plusieurs requêtes : données seulement quand toutes ont répondu, première erreur sinon. */
function combineAll<T>(results: UseQueryResult<T>[]) {
  const data = results.flatMap((result) => (result.data === undefined ? [] : [result.data]))
  return {
    data: data.length === results.length ? data : undefined,
    isPending: results.some((result) => result.isPending),
    error: results.find((result) => result.error)?.error ?? null,
    refetch: () => results.forEach((result) => void result.refetch()),
  }
}

/** Historique de la vue affichée (foyer ou membre). Clé partagée avec la vue d'ensemble. */
export function useViewHistory(range: HistoryRange) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['history', range, memberId],
    queryFn: () => api.history(range, { member_id: memberId }),
    placeholderData: keepPreviousData,
  })
}

/** Historique du foyer : l'avancement des objectifs se mesure toujours sur la part du foyer. */
export function useHouseholdHistory(range: HistoryRange) {
  return useQuery({
    queryKey: ['history', range, null],
    queryFn: () => api.history(range, { member_id: null }),
    placeholderData: keepPreviousData,
  })
}

export function useRealEstate() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['real-estate', memberId],
    queryFn: () => api.realEstate({ member_id: memberId }),
  })
}

/** Tableaux d'amortissement des prêts donnés, dans l'ordre des identifiants. */
export function useLoanSchedules(accountIds: readonly string[]) {
  const { memberId } = useView()
  return useQueries({
    queries: accountIds.map((id) => ({
      queryKey: ['loan-schedule', id, memberId],
      queryFn: () => api.loanSchedule(id, { member_id: memberId }),
    })),
    combine: combineAll,
  })
}

/** Détail (positions comprises) des comptes donnés. Clé partagée avec la fiche d'un compte. */
export function useAccountDetails(accountIds: readonly string[]) {
  const { memberId } = useView()
  return useQueries({
    queries: accountIds.map((id) => ({
      queryKey: ['account', id, memberId],
      queryFn: () => api.account(id, { member_id: memberId }),
    })),
    combine: combineAll,
  })
}
