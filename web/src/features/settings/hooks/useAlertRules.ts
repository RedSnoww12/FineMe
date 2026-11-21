import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { AlertRuleInput, AlertRuleKind } from '@/shared/api/types'

export function useAlertRules() {
  return useQuery({ queryKey: ['alert-rules'], queryFn: api.alertRules })
}

/** Modifie une règle ; les notifications sont relues, la prochaine évaluation en tenant compte. */
export function useUpdateAlertRule(onSuccess?: () => void) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ kind, input }: { kind: AlertRuleKind; input: AlertRuleInput }) => api.updateAlertRule(kind, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['alert-rules'] })
      void queryClient.invalidateQueries({ queryKey: ['alerts'] })
      onSuccess?.()
    },
  })
}
