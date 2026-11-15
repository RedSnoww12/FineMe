import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { GoalInput } from '@/shared/api/types'

/** Crée l'objectif, ou le met à jour quand `goalId` est fourni. */
export function useSaveGoal(goalId: string | undefined, onSuccess: () => void) {
  return useApiMutation(
    (input: GoalInput) => (goalId ? api.updateGoal(goalId, input) : api.createGoal(input)),
    onSuccess,
  )
}

export function useDeleteGoal(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteGoal(id), onSuccess)
}
