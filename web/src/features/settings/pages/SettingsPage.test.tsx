import { describe, expect, it } from 'vitest'
import { user } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, server, spyOn } from '@/test/server'

const CURRENT = 'ancien-mot-de-passe'
const NEXT = 'nouveau-mot-de-passe'

async function openSettings() {
  mockShell()
  const view = renderApp('/parametres')
  await screen.findByRole('heading', { name: 'Mot de passe' })
  return view
}

async function fillPasswordForm(actor: ReturnType<typeof renderApp>['user'], current: string, next: string, confirm = next) {
  await actor.type(screen.getByLabelText('Mot de passe actuel'), current)
  await actor.type(screen.getByLabelText('Nouveau mot de passe'), next)
  await actor.type(screen.getByLabelText('Confirmez le nouveau mot de passe'), confirm)
  await actor.click(screen.getByRole('button', { name: 'Changer le mot de passe' }))
}

describe('changement du mot de passe', () => {
  it('valide la saisie avant tout envoi', async () => {
    const calls = spyOn('put', '/api/auth/password')
    const { user: actor } = await openSettings()

    await actor.click(screen.getByRole('button', { name: 'Changer le mot de passe' }))
    expect(screen.getByText('Saisissez votre mot de passe actuel.')).toBeInTheDocument()
    expect(screen.getByText('Le mot de passe doit contenir au moins 10 caractères.')).toBeInTheDocument()

    await fillPasswordForm(actor, CURRENT, CURRENT)
    expect(screen.getByText('Le nouveau mot de passe doit être différent de l’actuel.')).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('envoie l’ancien et le nouveau mot de passe puis garde l’appareil connecté avec le nouveau jeton', async () => {
    let received: unknown
    let authorization: string | null = null
    server.use(
      http.put(`${API}/api/auth/password`, async ({ request }) => {
        received = await request.json()
        authorization = request.headers.get('authorization')
        return HttpResponse.json({ token: 'jeton-renouvele', user })
      }),
    )
    const { user: actor } = await openSettings()
    await fillPasswordForm(actor, CURRENT, NEXT)

    expect(await screen.findByText('Mot de passe modifié. Les autres appareils ont été déconnectés.')).toBeInTheDocument()
    expect(received).toEqual({ current_password: CURRENT, new_password: NEXT })
    expect(authorization).toBe('Bearer jeton-de-test')
    expect(localStorage.getItem('fineme.token')).toBe('jeton-renouvele')
    expect(screen.getByLabelText('Mot de passe actuel')).toHaveValue('')
    // Toujours sur l'écran : la session n'a pas été fermée.
    expect(screen.getByRole('heading', { name: 'Paramètres' })).toBeInTheDocument()
  })

  it('affiche le refus de l’API sans déconnecter', async () => {
    mockApiError('put', '/api/auth/password', 403, 'Mot de passe actuel incorrect')
    const { user: actor } = await openSettings()
    await fillPasswordForm(actor, 'mauvais-mot-de-passe', NEXT)

    expect(await screen.findByText('Mot de passe actuel incorrect')).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBe('jeton-de-test')
    expect(screen.getByRole('heading', { name: 'Paramètres' })).toBeInTheDocument()
  })
})
