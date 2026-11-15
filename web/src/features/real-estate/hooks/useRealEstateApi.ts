import { useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
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
