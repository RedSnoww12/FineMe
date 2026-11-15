import { QueryClient } from '@tanstack/react-query'
import { ApiError } from '@/shared/api/client'

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        // Une erreur 4xx est une réponse de l'API : la rejouer ne change rien.
        retry: (failureCount, error) =>
          !(error instanceof ApiError && error.status >= 400 && error.status < 500) && failureCount < 1,
      },
    },
  })
}
