import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { OAuthGrant } from '@/shared/api/types'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const grant = (overrides: Partial<OAuthGrant> = {}): OAuthGrant => ({
  id: 'autorisation-1',
  client_id: 'https://claude.ai/client.json',
  client_name: 'Claude',
  redirect_host: 'claude.ai',
  scope: 'read',
  created_at: '2026-11-01T10:00:00Z',
  last_used_at: null,
  revoked_at: null,
  ...overrides,
})

async function openSection({ totp = false, setup = () => {} } = {}) {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  mockJson('get', '/api/alerts/rules', [])
  if (totp) mockJson('get', '/api/auth/totp', { enabled: true, recovery_codes_remaining: 10 })
  setup()
  const view = renderApp('/parametres')
  const section = await screen.findByRole('region', { name: 'Connexions OAuth' })
  return { ...view, section: within(section) }
}

describe('Paramètres > Accès IA > connexions OAuth', () => {
  it('donne l’adresse du connecteur quand aucun client n’est autorisé', async () => {
    const { section } = await openSection()
    expect(await section.findByText('Aucune connexion OAuth pour le moment.')).toBeInTheDocument()
    expect(section.getByText(`${API}/mcp`)).toBeInTheDocument()
  })

  it('liste les clients autorisés et révoqués', async () => {
    const { section } = await openSection({
      setup: () =>
        mockJson('get', '/api/oauth/grants', [
          grant(),
          grant({ id: 'autorisation-2', client_name: 'Autre client', revoked_at: '2026-11-03T10:00:00Z' }),
        ]),
    })
    const list = within(await section.findByRole('list', { name: 'Clients autorisés' }))
    expect(list.getByText('Claude')).toBeInTheDocument()
    expect(list.getByText('Active')).toBeInTheDocument()
    expect(list.getByText('Révoquée')).toBeInTheDocument()
    expect(list.getAllByRole('button', { name: /Révoquer la connexion/ })).toHaveLength(1)
  })

  it('révoque avec le mot de passe et le code', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/oauth/grants/autorisation-1/revoke`, async ({ request }) => {
        received = await request.json()
        const revoked = grant({ revoked_at: '2026-11-03T10:00:00Z' })
        mockJson('get', '/api/oauth/grants', [revoked])
        return HttpResponse.json(revoked)
      }),
    )
    const { user, section } = await openSection({
      totp: true,
      setup: () => mockJson('get', '/api/oauth/grants', [grant()]),
    })
    await user.click(await section.findByRole('button', { name: 'Révoquer la connexion Claude' }))
    const form = within(section.getByRole('form', { name: 'Révoquer la connexion Claude' }))
    await user.type(form.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.type(form.getByLabelText('Code de vérification'), '123 456')
    await user.click(form.getByRole('button', { name: 'Révoquer' }))

    expect(await section.findByText('Révoquée')).toBeInTheDocument()
    expect(received).toEqual({ password: 'mot-de-passe-fictif', code: '123456' })
    expect(section.queryByRole('form')).not.toBeInTheDocument()
  })

  it('n’envoie rien sans mot de passe et affiche le refus de l’API', async () => {
    const calls = spyOn('post', '/api/oauth/grants/autorisation-1/revoke')
    const { user, section } = await openSection({
      setup: () => mockJson('get', '/api/oauth/grants', [grant()]),
    })
    await user.click(await section.findByRole('button', { name: 'Révoquer la connexion Claude' }))
    await user.click(section.getByRole('button', { name: 'Révoquer' }))
    expect(section.getByText('Saisissez votre mot de passe.')).toBeInTheDocument()
    expect(calls.count).toBe(0)

    mockApiError('post', '/api/oauth/grants/autorisation-1/revoke', 403, 'Mot de passe incorrect.')
    await user.type(section.getByLabelText('Mot de passe'), 'faux-mot-de-passe')
    await user.click(section.getByRole('button', { name: 'Révoquer' }))
    expect(await section.findByText('Mot de passe incorrect.')).toBeInTheDocument()
  })

  it('montre l’état réel quand la révocation est refusée car déjà faite', async () => {
    mockApiError('post', '/api/oauth/grants/autorisation-1/revoke', 409, 'Connexion déjà révoquée.')
    const { user, section } = await openSection({
      setup: () => mockJson('get', '/api/oauth/grants', [grant()]),
    })
    await user.click(await section.findByRole('button', { name: 'Révoquer la connexion Claude' }))
    mockJson('get', '/api/oauth/grants', [grant({ revoked_at: '2026-11-03T10:00:00Z' })])
    await user.type(section.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.click(section.getByRole('button', { name: 'Révoquer' }))
    expect(await section.findByText('Révoquée')).toBeInTheDocument()
    expect(section.queryByRole('form')).not.toBeInTheDocument()
  })

  it('rafraîchit la liste après un changement de mot de passe, qui révoque tout', async () => {
    mockJson('put', '/api/auth/password', { token: 'jeton-renouvele', user: { id: 'u', email: 'a@example.test' } })
    const { user, section } = await openSection({
      setup: () => mockJson('get', '/api/oauth/grants', [grant()]),
    })
    expect(await section.findByText('Active')).toBeInTheDocument()
    mockJson('get', '/api/oauth/grants', [grant({ revoked_at: '2026-11-03T10:00:00Z' })])
    await user.type(screen.getByLabelText('Mot de passe actuel'), 'ancien-mot-de-passe')
    await user.type(screen.getByLabelText('Nouveau mot de passe'), 'nouveau-mot-de-passe')
    await user.type(screen.getByLabelText('Confirmez le nouveau mot de passe'), 'nouveau-mot-de-passe')
    await user.click(screen.getByRole('button', { name: 'Changer le mot de passe' }))
    expect(await section.findByText('Révoquée')).toBeInTheDocument()
  })

  it('signale l’échec du chargement', async () => {
    const { section } = await openSection({
      setup: () => mockApiError('get', '/api/oauth/grants', 500, 'Erreur interne.'),
    })
    expect(await section.findByRole('alert')).toHaveTextContent('Erreur interne.')
  })
})
