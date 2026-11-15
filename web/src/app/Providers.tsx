import { QueryClientProvider, type QueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { AuthProvider } from '@/shared/state/auth'
import { ThemeProvider } from '@/shared/state/theme'
import { ViewProvider } from '@/shared/state/view'

/** Fournisseurs communs à l'application et aux tests d'intégration. Le routeur est fourni par l'appelant. */
export function Providers({ queryClient, children }: { queryClient: QueryClient; children: ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <AuthProvider>
          <ViewProvider>{children}</ViewProvider>
        </AuthProvider>
      </ThemeProvider>
    </QueryClientProvider>
  )
}
