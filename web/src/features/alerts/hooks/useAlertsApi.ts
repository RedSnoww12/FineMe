import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'

const ALERTS_KEY = ['alerts']

/** Alertes non archivées, relues régulièrement : la tâche quotidienne peut en ajouter sans action de l'utilisateur. */
export function useAlerts() {
  return useQuery({ queryKey: ALERTS_KEY, queryFn: () => api.alerts(), refetchInterval: 15 * 60_000 })
}

/** Mutation qui ne rafraîchit que les alertes (lire une alerte ne change aucun total). */
function useAlertsMutation<TVars, TData>(mutationFn: (vars: TVars) => Promise<TData>) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn,
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ALERTS_KEY }),
  })
}

export function useUpdateAlert() {
  return useAlertsMutation((vars: { id: string; read?: boolean; archived?: boolean }) => {
    const { id, ...body } = vars
    return api.updateAlert(id, body)
  })
}

export function useReadAllAlerts() {
  return useAlertsMutation(() => api.readAllAlerts())
}

export function useEvaluateAlerts() {
  return useAlertsMutation(() => api.evaluateAlerts())
}
