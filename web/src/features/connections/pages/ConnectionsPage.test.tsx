import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Connection, Provider } from '@/shared/api/types'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, mockUnreachable, server } from '@/test/server'

const aggregator: Provider = { id: 'agregateur-demo', name: 'Agrégateur Démo', configured: true }

function buildConnection(overrides: Partial<Connection> = {}): Connection {
  return {
    id: 'c-1',
    provider: aggregator.id,
    institution: 'Banque du Canal',
    status: 'active',
    last_sync_at: null,
    error: null,
    ...overrides,
  }
}

function mockConnections(providers: Provider[], connections: Connection[]) {
  mockShell()
  mockJson('get', '/api/connections/providers', providers)
  mockJson('get', '/api/connections', connections)
}

function connectionItem(institution: string) {
  const item = screen.getByText(institution).closest('li')
  if (!item) throw new Error(`Connexion ${institution} introuvable`)
  return item
}

describe('connexions bancaires', () => {
  it('oriente vers l’import CSV quand aucun agrégateur n’est configuré', async () => {
    mockConnections([], [])
    renderApp('/connexions')
    expect(await screen.findByRole('heading', { name: 'Aucun agrégateur bancaire n’est configuré' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Aller aux comptes' })).toHaveAttribute('href', '/comptes')
  })

  it('liste les agrégateurs et les banques connectées avec leur état', async () => {
    mockConnections(
      [aggregator, { id: 'autre', name: 'Agrégateur Inactif', configured: false }],
      [
        buildConnection(),
        buildConnection({ id: 'c-2', institution: null, status: 'error', error: 'Consentement révoqué par la banque.' }),
      ],
    )
    renderApp('/connexions')

    expect(await screen.findByText('Banque du Canal')).toBeInTheDocument()
    const buttons = screen.getAllByRole('button', { name: 'Connecter une banque' })
    expect(buttons[0]).toBeEnabled()
    expect(buttons[1]).toBeDisabled()
    expect(screen.getByText(/Non configuré sur le serveur/)).toBeInTheDocument()

    expect(pageText(connectionItem('Banque du Canal'))).toContain('Activevia Agrégateur Démo, jamais synchronisée')
    // Sans établissement connu, on affiche le nom de l'agrégateur.
    const failing = screen.getByText('En erreur').closest('li') as HTMLElement
    expect(pageText(failing)).toContain('Agrégateur Démo')
    expect(within(failing).getByRole('alert')).toHaveTextContent('Consentement révoqué par la banque.')
  })

  it('synchronise une banque et résume ce qui a été mis à jour', async () => {
    mockConnections([aggregator], [buildConnection()])
    const synced: string[] = []
    server.use(
      http.post(`${API}/api/connections/:id/sync`, ({ params }) => {
        synced.push(String(params.id))
        return HttpResponse.json({ accounts_updated: 2, transactions_inserted: 1 })
      }),
    )
    const { user: actor } = renderApp('/connexions')
    await screen.findByText('Banque du Canal')
    await actor.click(screen.getByRole('button', { name: 'Synchroniser' }))

    expect(await screen.findByText(/Synchronisation terminée/)).toHaveTextContent(
      'Synchronisation terminée : 2 comptes mis à jour, 1 opération ajoutée.',
    )
    expect(synced).toEqual(['c-1'])
  })

  it('affiche l’échec de la synchronisation sous la banque concernée', async () => {
    mockConnections([aggregator], [buildConnection()])
    mockApiError('post', '/api/connections/c-1/sync', 502, 'La banque ne répond pas.')
    const { user: actor } = renderApp('/connexions')
    await screen.findByText('Banque du Canal')
    await actor.click(screen.getByRole('button', { name: 'Synchroniser' }))
    expect(await within(connectionItem('Banque du Canal')).findByRole('alert')).toHaveTextContent('La banque ne répond pas.')
  })

  it('refuse de suivre une adresse de connexion qui n’est pas une adresse web', async () => {
    mockConnections([aggregator], [])
    let received: unknown
    server.use(
      http.post(`${API}/api/connections`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ id: 'c-new', redirect_url: 'javascript:alert(1)' })
      }),
    )
    const { user: actor } = renderApp('/connexions')
    await actor.click(await screen.findByRole('button', { name: 'Connecter une banque' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Le fournisseur n’a pas renvoyé d’adresse de connexion valide.')
    expect(received).toEqual({ provider: aggregator.id })
  })

  it('affiche le refus de l’API à la connexion', async () => {
    mockConnections([aggregator], [])
    mockApiError('post', '/api/connections', 501, 'Fournisseur non configuré.')
    const { user: actor } = renderApp('/connexions')
    expect(await screen.findByText('Aucune banque connectée pour l’instant.')).toBeInTheDocument()
    await actor.click(screen.getByRole('button', { name: 'Connecter une banque' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Fournisseur non configuré.')
  })

  it('supprime une connexion après confirmation', async () => {
    mockShell()
    mockJson('get', '/api/connections/providers', [aggregator])
    const state = { connections: [buildConnection()] }
    server.use(
      http.get(`${API}/api/connections`, () => HttpResponse.json(state.connections)),
      http.delete(`${API}/api/connections/c-1`, () => {
        state.connections = []
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const { user: actor } = renderApp('/connexions')
    await screen.findByText('Banque du Canal')
    await actor.click(screen.getByRole('button', { name: 'Supprimer' }))
    const dialog = await screen.findByRole('dialog', { name: 'Supprimer cette connexion ?' })
    expect(pageText(dialog)).toContain('La synchronisation avec « Banque du Canal » s’arrêtera.')
    await actor.click(within(dialog).getByRole('button', { name: 'Supprimer la connexion' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(await screen.findByText('Aucune banque connectée pour l’instant.')).toBeInTheDocument()
  })

  it('affiche un seul état d’erreur quand l’API est injoignable', async () => {
    mockShell()
    mockUnreachable('get', '/api/connections/providers')
    mockUnreachable('get', '/api/connections')
    renderApp('/connexions')
    expect(await screen.findByRole('alert')).toHaveTextContent('API injoignable')
    await waitFor(() => expect(screen.getAllByRole('alert')).toHaveLength(1))
  })
})
