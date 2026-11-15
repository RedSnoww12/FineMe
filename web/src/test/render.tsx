import { QueryClient } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import App from '@/app/App'
import { Providers } from '@/app/Providers'

type Options = {
  /** Simule une session ouverte en déposant un jeton avant le rendu. */
  authenticated?: boolean
}

/** Rend l'application réelle (routeur, shell, écrans) sur `route`, face à l'API simulée par MSW. */
export function renderApp(route: string, { authenticated = true }: Options = {}) {
  if (authenticated) localStorage.setItem('fineme.token', 'jeton-de-test')
  // Pas de nouvelle tentative : les tests d'erreur doivent répondre immédiatement.
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const view = render(
    <Providers queryClient={queryClient}>
      <MemoryRouter initialEntries={[route]}>
        <App />
      </MemoryRouter>
    </Providers>,
  )
  return { ...view, user: userEvent.setup() }
}

/** Texte de la page avec les espaces insécables du format fr-FR ramenées à des espaces simples. */
export function pageText(element: HTMLElement = document.body): string {
  return (element.textContent ?? '').replace(/[  ]/g, ' ')
}

export { screen }
