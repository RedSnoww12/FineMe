import { keepPreviousData, useMutation, useQueries, useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query'
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

/** Estimations datées des biens donnés, dans l'ordre des identifiants. */
export function useValuationsOf(accountIds: readonly string[]) {
  return useQueries({
    queries: accountIds.map((id) => ({
      queryKey: ['valuations', id],
      queryFn: () => api.valuations(id),
    })),
    combine: combineAll,
  })
}

/** Performance (TWR, MWR) de la vue sur la période ; désactivée tant que la période est invalide. */
export function usePerformance(bounds: { from?: string; to?: string } | null) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['performance', bounds, memberId],
    queryFn: () => api.performance(bounds ?? {}, { member_id: memberId }),
    enabled: bounds !== null,
    placeholderData: keepPreviousData,
  })
}

/** Choix ou retrait de l'indice de référence : la performance est relue ensuite. */
export function useBenchmarkMutations() {
  const queryClient = useQueryClient()
  const onSuccess = () => void queryClient.invalidateQueries({ queryKey: ['performance'] })
  return {
    save: useMutation({ mutationFn: api.setBenchmark, onSuccess }),
    remove: useMutation({ mutationFn: api.deleteBenchmark, onSuccess }),
  }
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

/** Historique complet de chaque membre donné, dans l'ordre des identifiants. Clé partagée avec la vue d'un membre. */
export function useMemberHistories(memberIds: readonly string[]) {
  return useQueries({
    queries: memberIds.map((id) => ({
      queryKey: ['history', 'all', id],
      queryFn: () => api.history('all', { member_id: id }),
    })),
    combine: combineAll,
  })
}
