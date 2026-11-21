import { waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildOverview, user } from '@/test/fixtures'
import { mockOverviewScreen, mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, mockUnreachable, server, spyOn } from '@/test/server'

const PASSWORD = 'tournesol-violet-42'

describe('première configuration', () => {
  it('propose la création du compte quand aucun utilisateur n’existe', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true })
    renderApp('/connexion', { authenticated: false })
    expect(await screen.findByRole('heading', { name: 'Première configuration' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Connexion' })).not.toBeInTheDocument()
  })

  it('valide le formulaire avant tout envoi', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true })
    const setupCalls = spyOn('post', '/api/auth/setup')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await screen.findByRole('heading', { name: 'Première configuration' })

    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(screen.getByText('Indiquez votre prénom ou votre nom.')).toBeInTheDocument()
    expect(screen.getByText('Saisissez une adresse e-mail valide.')).toBeInTheDocument()
    expect(screen.getByText('Le mot de passe doit contenir au moins 10 caractères.')).toBeInTheDocument()

    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.type(screen.getByLabelText('Confirmez le mot de passe'), 'autre-chose-123')
    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(screen.getByText('Les deux mots de passe ne correspondent pas.')).toBeInTheDocument()
    expect(setupCalls.count).toBe(0)
  })

  it('crée le compte, mémorise le jeton et ouvre la vue d’ensemble', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true })
    let received: unknown
    server.use(
      http.post(`${API}/api/auth/setup`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ token: 'jeton-setup', user })
      }),
    )
    mockShell()
    mockOverviewScreen(buildOverview())

    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await actor.type(await screen.findByLabelText('Votre nom'), user.name)
    await actor.type(screen.getByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.type(screen.getByLabelText('Confirmez le mot de passe'), PASSWORD)
    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))

    expect(await screen.findByRole('heading', { name: 'Vue d’ensemble' })).toBeInTheDocument()
    expect(received).toEqual({ name: user.name, email: user.email, password: PASSWORD })
    expect(localStorage.getItem('fineme.token')).toBe('jeton-setup')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))
  })

  it('affiche le message de l’API quand la création est refusée', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true })
    mockApiError('post', '/api/auth/setup', 409, 'Un utilisateur existe déjà.')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await actor.type(await screen.findByLabelText('Votre nom'), user.name)
    await actor.type(screen.getByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.type(screen.getByLabelText('Confirmez le mot de passe'), PASSWORD)
    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(await screen.findByText('Un utilisateur existe déjà.')).toBeInTheDocument()
  })
})

describe('connexion', () => {
  it('affiche le formulaire de connexion quand un utilisateur existe', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false })
    renderApp('/connexion', { authenticated: false })
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
    expect(screen.queryByLabelText('Confirmez le mot de passe')).not.toBeInTheDocument()
  })

  it('redirige vers la connexion quand on ouvre un écran sans session', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false })
    renderApp('/objectifs', { authenticated: false })
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
  })

  it('montre l’erreur de l’API pour des identifiants refusés', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false })
    mockApiError('post', '/api/auth/login', 401, 'E-mail ou mot de passe incorrect.')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await actor.type(await screen.findByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), 'mauvais-mot-de-passe')
    await actor.click(screen.getByRole('button', { name: 'Se connecter' }))
    expect(await screen.findByText('E-mail ou mot de passe incorrect.')).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBeNull()
  })

  it('affiche le délai d’attente et bloque l’envoi après trop de tentatives', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false })
    server.use(
      http.post(`${API}/api/auth/login`, () =>
        HttpResponse.json(
          { error: 'Trop de tentatives. Réessayez dans 30 secondes.' },
          { status: 429, headers: { 'Retry-After': '30' } },
        ),
      ),
    )
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await actor.type(await screen.findByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.click(screen.getByRole('button', { name: 'Se connecter' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Trop de tentatives. Réessayez dans 30 secondes.')
    const button = await screen.findByRole('button', { name: /Réessayer dans (30|29) s/ })
    expect(button).toBeDisabled()
  })

  it('connecte l’utilisateur et envoie le jeton sur les requêtes suivantes', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false })
    mockJson('post', '/api/auth/login', { token: 'jeton-login', user })
    mockShell()
    mockJson('get', '/api/history', { points: [] })
    let authorization: string | null = null
    server.use(
      http.get(`${API}/api/overview`, ({ request }) => {
        authorization = request.headers.get('authorization')
        return HttpResponse.json(buildOverview())
      }),
    )

    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await actor.type(await screen.findByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.click(screen.getByRole('button', { name: 'Se connecter' }))

    expect(await screen.findByRole('heading', { name: 'Vue d’ensemble' })).toBeInTheDocument()
    await waitFor(() => expect(authorization).toBe('Bearer jeton-login'))
  })
})

describe('API injoignable', () => {
  it('l’annonce clairement et permet de réessayer', async () => {
    mockUnreachable('get', '/api/auth/status')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('API injoignable')

    mockJson('get', '/api/auth/status', { setup_required: false })
    await actor.click(screen.getByRole('button', { name: 'Réessayer' }))
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
  })

  it('traite une erreur 502 sans corps JSON comme une API injoignable', async () => {
    server.use(http.get(`${API}/api/auth/status`, () => new HttpResponse('Bad Gateway', { status: 502 })))
    renderApp('/connexion', { authenticated: false })
    expect(await screen.findByRole('alert')).toHaveTextContent('API injoignable')
  })
})

describe('code de configuration', () => {
  async function fillSetupForm(actor: ReturnType<typeof renderApp>['user']) {
    await actor.type(await screen.findByLabelText('Votre nom'), user.name)
    await actor.type(screen.getByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Mot de passe'), PASSWORD)
    await actor.type(screen.getByLabelText('Confirmez le mot de passe'), PASSWORD)
  }

  /** Capture le corps du POST /api/auth/setup et répond par une session valide. */
  function captureSetup() {
    const captured: { body?: unknown } = {}
    server.use(
      http.post(`${API}/api/auth/setup`, async ({ request }) => {
        captured.body = await request.json()
        return HttpResponse.json({ token: 'jeton-setup', user })
      }),
    )
    mockShell()
    mockOverviewScreen(buildOverview())
    return captured
  }

  it.each([
    ['false', { setup_required: true, setup_token_required: false }],
    ['absent de la réponse', { setup_required: true }],
  ])('masque le champ et n’envoie pas setup_token quand setup_token_required est %s', async (_case, status) => {
    mockJson('get', '/api/auth/status', status)
    const captured = captureSetup()
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await fillSetupForm(actor)
    expect(screen.queryByLabelText('Code de configuration')).not.toBeInTheDocument()

    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(await screen.findByRole('heading', { name: 'Vue d’ensemble' })).toBeInTheDocument()
    expect(captured.body).toEqual({ name: user.name, email: user.email, password: PASSWORD })
    expect(captured.body).not.toHaveProperty('setup_token')
  })

  it('affiche le champ, l’exige et envoie sa valeur quand le serveur le demande', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true, setup_token_required: true })
    const captured = captureSetup()
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await fillSetupForm(actor)

    const field = screen.getByLabelText('Code de configuration')
    expect(field).toHaveAccessibleDescription(/SETUP_TOKEN/)

    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(screen.getByText('Saisissez le code de configuration.')).toBeInTheDocument()
    expect(captured.body).toBeUndefined()

    await actor.type(field, 'code-fictif-123')
    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))
    expect(await screen.findByRole('heading', { name: 'Vue d’ensemble' })).toBeInTheDocument()
    expect(captured.body).toEqual({
      name: user.name,
      email: user.email,
      password: PASSWORD,
      setup_token: 'code-fictif-123',
    })
  })

  it('affiche le message 403 de l’API quand le code est refusé', async () => {
    mockJson('get', '/api/auth/status', { setup_required: true, setup_token_required: true })
    mockApiError('post', '/api/auth/setup', 403, 'Code de configuration invalide.')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await fillSetupForm(actor)
    await actor.type(screen.getByLabelText('Code de configuration'), 'mauvais-code')
    await actor.click(screen.getByRole('button', { name: 'Créer mon compte' }))

    expect(await screen.findByText('Code de configuration invalide.')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Première configuration' })).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBeNull()
  })
})

describe('mot de passe oublié', () => {
  const RECOVERY = 'code-de-secours-fictif'

  async function openResetForm(actor: ReturnType<typeof renderApp>['user']) {
    await actor.click(await screen.findByRole('button', { name: 'Mot de passe oublié ?' }))
    expect(screen.getByRole('heading', { name: 'Mot de passe oublié' })).toBeInTheDocument()
  }

  async function fillResetForm(actor: ReturnType<typeof renderApp>['user'], code = RECOVERY) {
    await actor.type(screen.getByLabelText('Adresse e-mail'), user.email)
    await actor.type(screen.getByLabelText('Code de secours'), code)
    await actor.type(screen.getByLabelText('Nouveau mot de passe'), PASSWORD)
    await actor.type(screen.getByLabelText('Confirmez le nouveau mot de passe'), PASSWORD)
    await actor.click(screen.getByRole('button', { name: 'Réinitialiser le mot de passe' }))
  }

  it.each([
    ['false', { setup_required: false, recovery_enabled: false }],
    ['absent de la réponse', { setup_required: false }],
  ])('masque le lien quand recovery_enabled est %s', async (_case, status) => {
    mockJson('get', '/api/auth/status', status)
    renderApp('/connexion', { authenticated: false })
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Mot de passe oublié ?' })).not.toBeInTheDocument()
  })

  it('valide la saisie avant tout envoi et permet de revenir à la connexion', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false, recovery_enabled: true })
    const calls = spyOn('post', '/api/auth/reset')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await openResetForm(actor)

    expect(screen.getByLabelText('Code de secours')).toHaveAccessibleDescription(/RECOVERY_TOKEN/)
    await actor.click(screen.getByRole('button', { name: 'Réinitialiser le mot de passe' }))
    expect(screen.getByText('Saisissez une adresse e-mail valide.')).toBeInTheDocument()
    expect(screen.getByText('Saisissez le code de secours.')).toBeInTheDocument()
    expect(screen.getByText('Le mot de passe doit contenir au moins 10 caractères.')).toBeInTheDocument()
    expect(calls.count).toBe(0)

    await actor.click(screen.getByRole('button', { name: 'Retour à la connexion' }))
    expect(screen.getByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
  })

  it('réinitialise le mot de passe, ouvre la session et la vue d’ensemble', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false, recovery_enabled: true })
    let received: unknown
    server.use(
      http.post(`${API}/api/auth/reset`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ token: 'jeton-reset', user })
      }),
    )
    mockShell()
    mockOverviewScreen(buildOverview())
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await openResetForm(actor)
    await fillResetForm(actor, ` ${RECOVERY} `)

    expect(await screen.findByRole('heading', { name: 'Vue d’ensemble' })).toBeInTheDocument()
    expect(received).toEqual({ email: user.email, recovery_token: RECOVERY, new_password: PASSWORD })
    expect(localStorage.getItem('fineme.token')).toBe('jeton-reset')
  })

  it('affiche le message de l’API quand le code est refusé', async () => {
    mockJson('get', '/api/auth/status', { setup_required: false, recovery_enabled: true })
    mockApiError('post', '/api/auth/reset', 403, 'Code de secours invalide ou manquant')
    const { user: actor } = renderApp('/connexion', { authenticated: false })
    await openResetForm(actor)
    await fillResetForm(actor, 'mauvais-code')

    expect(await screen.findByText('Code de secours invalide ou manquant')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Mot de passe oublié' })).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBeNull()
  })
})
