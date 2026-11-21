import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { user } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

const RECOVERY_CODES = Array.from({ length: 10 }, (_, i) => `abcde-fgh${i}${i}`.slice(0, 11))
const ENROLLMENT = {
  secret: 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
  otpauth_uri: 'otpauth://totp/FineMe:camille%40exemple.test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
  qr_svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><rect width="21" height="21"/></svg>',
}

async function openSettings(enabled = false, remaining = 0) {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  mockJson('get', '/api/alerts/rules', [])
  mockJson('get', '/api/auth/totp', { enabled, recovery_codes_remaining: remaining })
  const view = renderApp('/parametres')
  const panel = (await screen.findByRole('heading', { name: 'Vérification en deux étapes' })).closest('section')
  if (!panel) throw new Error('panneau introuvable')
  return { ...view, panel: within(panel) }
}

describe('activation du second facteur', () => {
  it('affiche le QR code, confirme par un premier code puis montre les codes de secours', async () => {
    mockJson('post', '/api/auth/totp/setup', ENROLLMENT)
    let received: unknown
    server.use(
      http.post(`${API}/api/auth/totp/enable`, async ({ request }) => {
        received = await request.json()
        // Après l'activation, l'état relu est « activé ».
        mockJson('get', '/api/auth/totp', { enabled: true, recovery_codes_remaining: 10 })
        return HttpResponse.json({ token: 'jeton-totp', user, recovery_codes: RECOVERY_CODES })
      }),
    )
    const { user: actor, panel } = await openSettings()
    expect(await panel.findByText('Désactivée')).toBeInTheDocument()
    await actor.click(panel.getByRole('button', { name: 'Activer la vérification en deux étapes' }))

    const qr = await panel.findByRole('img', { name: /QR code/ })
    expect(qr.getAttribute('src')).toMatch(/^data:image\/svg\+xml;charset=utf-8,%3Csvg/)
    expect(panel.getByText('GEZD GNBV GY3T QOJQ GEZD GNBV GY3T QOJQ')).toBeInTheDocument()

    await actor.click(panel.getByRole('button', { name: 'Activer' }))
    expect(panel.getByText('Saisissez le code de votre application.')).toBeInTheDocument()
    await actor.type(panel.getByLabelText('Code de l’application'), '123 456')
    await actor.click(panel.getByRole('button', { name: 'Activer' }))

    const list = await panel.findByRole('list', { name: 'Codes de secours' })
    expect(within(list).getAllByRole('listitem')).toHaveLength(10)
    expect(received).toEqual({ code: '123456' })
    // Les autres sessions sont fermées : cet appareil garde la sienne avec le nouveau jeton.
    expect(localStorage.getItem('fineme.token')).toBe('jeton-totp')

    await actor.click(panel.getByRole('button', { name: 'J’ai noté mes codes' }))
    expect(await panel.findByText('Activée')).toBeInTheDocument()
    expect(panel.getByText(/10 codes de secours restants/)).toBeInTheDocument()
  })

  it('affiche le refus d’un mauvais code sans activer', async () => {
    mockJson('post', '/api/auth/totp/setup', ENROLLMENT)
    mockApiError('post', '/api/auth/totp/enable', 403, 'Code de vérification incorrect')
    const { user: actor, panel } = await openSettings()
    await actor.click(await panel.findByRole('button', { name: 'Activer la vérification en deux étapes' }))
    await actor.type(await panel.findByLabelText('Code de l’application'), '000000')
    await actor.click(panel.getByRole('button', { name: 'Activer' }))
    expect(await panel.findByText('Code de vérification incorrect')).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBe('jeton-de-test')
  })

  it('montre une erreur quand l’état du second facteur est indisponible', async () => {
    mockShell()
    mockJson('get', '/api/auth/logins', [])
    mockJson('get', '/api/alerts/rules', [])
    mockApiError('get', '/api/auth/totp', 500, 'Erreur interne du serveur')
    renderApp('/parametres')
    expect(await screen.findByText('Erreur interne du serveur')).toBeInTheDocument()
  })
})

describe('second facteur activé', () => {
  it('désactive avec le mot de passe et un code', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/auth/totp/disable`, async ({ request }) => {
        received = await request.json()
        mockJson('get', '/api/auth/totp', { enabled: false, recovery_codes_remaining: 0 })
        return HttpResponse.json({ token: 'jeton-sans-totp', user })
      }),
    )
    const { user: actor, panel } = await openSettings(true, 7)
    expect(await panel.findByText(/7 codes de secours restants/)).toBeInTheDocument()
    await actor.click(panel.getByRole('button', { name: 'Désactiver' }))
    expect(panel.getByText('Saisissez votre mot de passe.')).toBeInTheDocument()

    await actor.type(panel.getByLabelText('Mot de passe'), 'mot-de-passe-fictif')
    await actor.type(panel.getByLabelText('Code de vérification'), 'abcde-fgh89')
    await actor.click(panel.getByRole('button', { name: 'Désactiver' }))

    expect(await panel.findByText('Désactivée')).toBeInTheDocument()
    expect(received).toEqual({ password: 'mot-de-passe-fictif', code: 'abcde-fgh89' })
    expect(localStorage.getItem('fineme.token')).toBe('jeton-sans-totp')
  })

  it('régénère les codes de secours avec un code de l’application', async () => {
    mockJson('post', '/api/auth/totp/recovery-codes', { recovery_codes: RECOVERY_CODES })
    const { user: actor, panel } = await openSettings(true, 2)
    await actor.type(await panel.findByLabelText('Code de vérification'), '654321')
    await actor.click(panel.getByRole('button', { name: 'Régénérer les codes de secours' }))
    const list = await panel.findByRole('list', { name: 'Codes de secours' })
    expect(within(list).getAllByRole('listitem')).toHaveLength(10)
  })
})
