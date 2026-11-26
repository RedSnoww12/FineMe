import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { AccessToken, McpCall } from '@/shared/api/types'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const SECRET = 'fm_pat_jetonFictifPourLesTests0123456789abcdefg'

const token = (overrides: Partial<AccessToken> = {}): AccessToken => ({
  id: 'jeton-1',
  name: 'Claude Code',
  scope: 'read',
  hint: 'fm_pat_…defg',
  created_at: '2026-11-01T10:00:00Z',
  last_used_at: null,
  revoked_at: null,
  ...overrides,
})

const call = (overrides: Partial<McpCall> = {}): McpCall => ({
  at: '2026-11-02T09:30:00Z',
  token_id: 'jeton-1',
  token_name: 'Claude Code',
  tool: 'get_overview',
  success: true,
  ...overrides,
})

/** Ouvre les Paramètres ; `setup` ajoute les réponses propres au test, après celles du shell. */
async function openPanel({ totp = false, setup = () => {} } = {}) {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  mockJson('get', '/api/alerts/rules', [])
  if (totp)
    mockJson('get', '/api/auth/totp', {
      enabled: true,
      recovery_codes_remaining: 10,
    })
  setup()
  const view = renderApp('/parametres')
  const section = (await screen.findByRole('heading', { name: 'Accès IA' })).closest('section')
  if (!section) throw new Error('panneau Accès IA introuvable')
  return { ...view, panel: within(section) }
}

describe('Paramètres > Accès IA', () => {
  it('crée un jeton, l’affiche une seule fois avec la commande Claude Code, puis le liste', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/access-tokens`, async ({ request }) => {
        received = await request.json()
        mockJson('get', '/api/access-tokens', [token()])
        return HttpResponse.json({ token: token(), secret: SECRET }, { status: 201 })
      }),
    )
    const { user, panel } = await openPanel()
    expect(await panel.findByText('Aucun jeton pour le moment.')).toBeInTheDocument()
    expect(panel.queryByLabelText('Code de vérification')).not.toBeInTheDocument()

    await user.type(panel.getByLabelText('Nom du jeton'), 'Claude Code')
    await user.type(panel.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.click(panel.getByRole('button', { name: 'Créer le jeton' }))

    const command = await panel.findByLabelText('Commande Claude Code')
    expect(command).toHaveTextContent(
      `claude mcp add --transport http fineme ${API}/mcp --header "Authorization: Bearer ${SECRET}"`,
    )
    expect(received).toEqual({
      name: 'Claude Code',
      password: 'mot-de-passe-fictif',
    })
    expect(panel.getByText(/il ne sera plus jamais affiché/)).toBeInTheDocument()

    await user.click(panel.getByRole('button', { name: 'J’ai copié le jeton' }))
    expect(panel.queryByText(SECRET, { exact: false })).not.toBeInTheDocument()
    const list = await panel.findByRole('list', { name: 'Jetons d’accès' })
    expect(within(list).getByText('Claude Code')).toBeInTheDocument()
    expect(within(list).getByText('Actif')).toBeInTheDocument()
    expect(within(list).getByText(/jamais utilisé/)).toBeInTheDocument()
  })

  it('n’envoie rien sans nom ni mot de passe', async () => {
    const calls = spyOn('post', '/api/access-tokens')
    const { user, panel } = await openPanel()
    await user.click(await panel.findByRole('button', { name: 'Créer le jeton' }))
    expect(panel.getByText(/Donnez un nom au jeton/)).toBeInTheDocument()
    expect(panel.getByText('Saisissez votre mot de passe.')).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('exige le code du second facteur quand il est activé et affiche le refus de l’API', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/access-tokens`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ error: 'Code de vérification incorrect' }, { status: 403 })
      }),
    )
    const { user, panel } = await openPanel({ totp: true })
    await user.type(await panel.findByLabelText('Nom du jeton'), 'Bureau')
    await user.type(panel.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.click(panel.getByRole('button', { name: 'Créer le jeton' }))
    expect(panel.getByText(/Saisissez un code de l’application/)).toBeInTheDocument()

    await user.type(panel.getByLabelText('Code de vérification'), '123 456')
    await user.click(panel.getByRole('button', { name: 'Créer le jeton' }))
    expect(await panel.findByText('Code de vérification incorrect')).toBeInTheDocument()
    expect(received).toEqual({
      name: 'Bureau',
      password: 'mot-de-passe-fictif',
      code: '123456',
    })
  })

  it('révoque un jeton après confirmation du mot de passe', async () => {
    let received: unknown
    const revokeHandler = () =>
      server.use(
        http.post(`${API}/api/access-tokens/jeton-1/revoke`, async ({ request }) => {
          received = await request.json()
          const revoked = token({ revoked_at: '2026-11-03T08:00:00Z' })
          mockJson('get', '/api/access-tokens', [revoked])
          return HttpResponse.json(revoked)
        }),
      )
    const { user, panel } = await openPanel({
      setup: () => {
        mockJson('get', '/api/access-tokens', [token({ last_used_at: '2026-11-02T09:30:00Z' })])
        revokeHandler()
      },
    })
    const list = await panel.findByRole('list', { name: 'Jetons d’accès' })
    expect(within(list).getByText(/utilisé le/)).toBeInTheDocument()
    await user.click(within(list).getByRole('button', { name: 'Révoquer Claude Code' }))
    const form = within(list).getByRole('form', {
      name: 'Révoquer Claude Code',
    })
    await user.type(within(form).getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.click(within(form).getByRole('button', { name: 'Révoquer' }))

    expect(await within(list).findByText('Révoqué')).toBeInTheDocument()
    expect(received).toEqual({ password: 'mot-de-passe-fictif' })
    expect(within(list).queryByRole('button', { name: 'Révoquer Claude Code' })).not.toBeInTheDocument()
  })

  it('montre le journal des appels avec leur issue', async () => {
    const { panel } = await openPanel({
      setup: () => {
        mockJson('get', '/api/access-tokens', [token()])
        mockJson('get', '/api/access-tokens/calls', [call({ tool: 'compare_dates', success: false }), call()])
      },
    })
    const journal = await panel.findByRole('list', { name: 'Appels récents' })
    const rows = within(journal).getAllByRole('listitem')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('Comparaison de dates')
    expect(rows[0]).toHaveTextContent('Erreur')
    expect(rows[1]).toHaveTextContent('Vue d’ensemble')
    expect(panel.getByText('2 appels, dont 1 en erreur récents.')).toBeInTheDocument()
  })

  it('remplace le formulaire au-delà de 10 jetons actifs ; un jeton révoqué ne compte pas', async () => {
    const tokens = (revoked: number) =>
      Array.from({ length: 10 }, (_, i) =>
        token({ id: `jeton-${i}`, name: `Jeton ${i}`, revoked_at: i < revoked ? '2026-11-03T08:00:00Z' : null }),
      )
    const full = await openPanel({ setup: () => mockJson('get', '/api/access-tokens', tokens(0)) })
    expect(await full.panel.findByText(/10 jetons actifs : révoquez-en un/)).toBeInTheDocument()
    expect(full.panel.queryByRole('button', { name: 'Créer le jeton' })).not.toBeInTheDocument()
    full.unmount()

    const room = await openPanel({ setup: () => mockJson('get', '/api/access-tokens', tokens(1)) })
    expect(await room.panel.findByRole('button', { name: 'Créer le jeton' })).toBeInTheDocument()
  })

  it('dit quand la copie de la commande est impossible', async () => {
    mockJson('post', '/api/access-tokens', { token: token(), secret: SECRET }, 201)
    const { user, panel } = await openPanel()
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true })
    await user.type(await panel.findByLabelText('Nom du jeton'), 'Claude Code')
    await user.type(panel.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await user.click(panel.getByRole('button', { name: 'Créer le jeton' }))
    await user.click(await panel.findByRole('button', { name: 'Copier la commande' }))
    expect(panel.getByRole('alert')).toHaveTextContent(/sélectionnez la commande/)
  })

  it('affiche l’erreur quand les jetons sont indisponibles', async () => {
    const { panel } = await openPanel({
      setup: () => mockApiError('get', '/api/access-tokens', 500, 'Erreur interne du serveur'),
    })
    expect(await panel.findByText('Erreur interne du serveur')).toBeInTheDocument()
  })
})
