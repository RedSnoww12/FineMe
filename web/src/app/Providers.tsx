import { QueryClientProvider, type QueryClient } from '@tanstack/react-query'
import { PersistQueryClientProvider, type Persister } from '@tanstack/react-query-persist-client'
import type { ReactNode } from 'react'
import { PERSISTED_CACHE_BUSTER, PERSISTED_CACHE_MAX_AGE_MS } from '@/shared/api/persistence'
import { AuthProvider } from '@/shared/state/auth'
import { DiscreetProvider } from '@/shared/state/discreet'
import { ThemeProvider } from '@/shared/state/theme'
import { ViewProvider } from '@/shared/state/view'

type Props = {
  queryClient: QueryClient
  /** Copie locale du cache pour l'affichage hors ligne ; absente, le cache ne vit qu'en mémoire. */
  persister?: Persister
  children: ReactNode
}

function QueryProvider({ queryClient, persister, children }: Omit<Props, 'children'> & { children: ReactNode }) {
  if (!persister) return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  return (
    <PersistQueryClientProvider
      client={queryClient}
      persistOptions={{ persister, maxAge: PERSISTED_CACHE_MAX_AGE_MS, buster: PERSISTED_CACHE_BUSTER }}
    >
      {children}
    </PersistQueryClientProvider>
  )
}

/** Fournisseurs communs à l'application et aux tests d'intégration. Le routeur est fourni par l'appelant. */
export function Providers({ queryClient, persister, children }: Props) {
  return (
    <QueryProvider queryClient={queryClient} persister={persister}>
      <ThemeProvider>
        <AuthProvider>
          <DiscreetProvider>
            <ViewProvider>{children}</ViewProvider>
          </DiscreetProvider>
        </AuthProvider>
      </ThemeProvider>
    </QueryProvider>
  )
}
