import { QueryClient } from '@tanstack/react-query'
import { ApiError } from '@/shared/api/client'

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        // Une erreur 4xx est une réponse de l'API : la rejouer ne change rien. Les autres (réseau, 502/503 de
        // l'hébergeur pendant le réveil de l'API) sont rejouées trois fois, à 1, 2 puis 4 s d'intervalle.
        retry: (failureCount, error) =>
          !(error instanceof ApiError && error.status >= 400 && error.status < 500) && failureCount < 3,
      },
    },
  })
}
