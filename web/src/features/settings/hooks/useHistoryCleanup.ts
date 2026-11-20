import { useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type {
  HistoryCorrectionInput,
  HistoryDeleteResult,
  HistoryRecomputeResult,
  HistoryReviewPoint,
} from '@/shared/api/types'
import { useView } from '@/shared/state/view'

// Chaque écriture passe par `useApiMutation`, qui invalide toutes les lectures : les requêtes `history`
// des graphiques (vue d'ensemble, analyse) et `history-review` de cet écran se rechargent aussitôt.

/** Points de l'historique de la vue affichée, avec les points suspects signalés par l'API. */
export function useHistoryReview() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['history-review', memberId],
    queryFn: () => api.historyReview({ member_id: memberId }),
  })
}

/** Corrige un point de la vue affichée. */
export function useCorrectHistoryPoint(onSuccess: (point: HistoryReviewPoint) => void) {
  const { memberId } = useView()
  return useApiMutation(
    ({ date, input }: { date: string; input: HistoryCorrectionInput }) =>
      api.correctHistoryPoint(date, { member_id: memberId }, input),
    onSuccess,
  )
}

/** Supprime une période (un seul jour si `from = to`), pour toutes les vues. */
export function useDeleteHistoryRange(onSuccess: (result: HistoryDeleteResult) => void) {
  return useApiMutation((range: { from: string; to: string }) => api.deleteHistoryRange(range), onSuccess)
}

export function useRecomputeHistory(onSuccess: (result: HistoryRecomputeResult) => void) {
  return useApiMutation<void, HistoryRecomputeResult>(() => api.recomputeHistory(), onSuccess)
}

export function useResetHistory(onSuccess: (result: HistoryDeleteResult) => void) {
  return useApiMutation((confirmation: string) => api.resetHistory(confirmation), onSuccess)
}
