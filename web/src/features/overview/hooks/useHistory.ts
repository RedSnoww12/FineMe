import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { HistoryRange } from '@/shared/api/types'
import { useView } from '@/shared/state/view'

export function useHistory(range: HistoryRange) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['history', range, memberId],
    queryFn: () => api.history(range, { member_id: memberId }),
    placeholderData: keepPreviousData,
  })
}
