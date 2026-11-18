import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ErrorBoundary } from './ErrorBoundary'

function Broken(): never {
  throw new Error('rendu impossible')
}

describe('ErrorBoundary', () => {
  afterEach(() => vi.restoreAllMocks())

  it('remplace un écran qui plante par un message au lieu d’une page blanche', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    render(
      <ErrorBoundary>
        <Broken />
      </ErrorBoundary>,
    )
    expect(screen.getByRole('alert')).toHaveTextContent('Cet écran n’a pas pu s’afficher')
    expect(screen.getByRole('button', { name: 'Recharger la page' })).toBeInTheDocument()
  })

  it('affiche son contenu quand tout va bien', () => {
    render(
      <ErrorBoundary>
        <p>Contenu</p>
      </ErrorBoundary>,
    )
    expect(screen.getByText('Contenu')).toBeInTheDocument()
  })
})
