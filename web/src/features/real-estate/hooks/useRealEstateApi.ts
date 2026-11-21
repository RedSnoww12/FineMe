import { useQueries, useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { LoanSchedule, ValuationInput } from '@/shared/api/types'
import { useView } from '@/shared/state/view'

export function useRealEstate() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['real-estate', memberId],
    queryFn: () => api.realEstate({ member_id: memberId }),
  })
}

export function useLoanSchedule(accountId: string | null) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['loan-schedule', accountId, memberId],
    queryFn: () => api.loanSchedule(accountId as string, { member_id: memberId }),
    enabled: Boolean(accountId),
  })
}

/** Estimations datées d'un bien, de la plus récente à la plus ancienne. */
export function useValuations(accountId: string) {
  return useQuery({
    queryKey: ['valuations', accountId],
    queryFn: () => api.valuations(accountId),
  })
}

/** Tableaux d'amortissement des prêts liés à un bien (montants à 100 %). */
export function useLinkedSchedules(accountIds: readonly string[]) {
  return useQueries({
    queries: accountIds.map((id) => ({
      queryKey: ['loan-schedule', id, null],
      queryFn: () => api.loanSchedule(id, { member_id: null }),
    })),
    combine: (results) => ({
      data: results.every((r) => r.isSuccess) ? results.map((r) => r.data as LoanSchedule) : undefined,
      isPending: results.some((r) => r.isPending),
      error: results.find((r) => r.error)?.error ?? null,
      refetch: () => results.forEach((r) => void r.refetch()),
    }),
  })
}

export function useAddValuation(accountId: string, onSuccess: () => void) {
  return useApiMutation((input: ValuationInput) => api.createValuation(accountId, input), onSuccess)
}

export function useDeleteValuation(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteValuation(id), onSuccess)
}
