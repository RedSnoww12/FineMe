import { waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildOverview, camille, emptyOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, mockUnreachable, server } from '@/test/server'

const household = buildOverview()
const camilleOnly = buildOverview({ gross_assets: 98_500, liabilities: 30_000, net_worth: 68_500 })

/** Répond selon `member_id` et consigne les valeurs reçues. */
function mockOverviewByMember() {
  const seen: (string | null)[] = []
  server.use(
    http.get(`${API}/api/overview`, ({ request }) => {
      const memberId = new URL(request.url).searchParams.get('member_id')
      seen.push(memberId)
      return HttpResponse.json(memberId === camille.id ? camilleOnly : household)
    }),
    http.get(`${API}/api/history`, ({ request }) => {
      seen.push(`history:${new URL(request.url).searchParams.get('member_id')}`)
      return HttpResponse.json({ points: [] })
    }),
  )
  return seen
}

describe('vue d’ensemble dans le shell', () => {
  it('affiche le patrimoine du foyer renvoyé par l’API', async () => {
    mockShell()
    const seen = mockOverviewByMember()
    renderApp('/')

    await waitFor(() => expect(pageText()).toContain('120 000 €'))
    const text = pageText()
    expect(text).toContain('Ensemble du foyer')
    expect(text).toContain('180 000 €') // actifs bruts
    expect(text).toContain('60 000 €') // dettes
    expect(text).toContain('45 000 €') // actifs financiers
    expect(text).toContain('+3 000 €') // plus-value latente
    expect(text).toContain('Fonds Indice Monde')
    expect(text).toContain('0,25 %') // frais courants moyens (fraction 0,0025)
    expect(seen).toContain(null)
    expect(seen).not.toContain(camille.id)
  })

  it('envoie member_id quand on choisit un membre, puis revient au foyer', async () => {
    mockShell()
    const seen = mockOverviewByMember()
    const { user } = renderApp('/')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))

    const [switcher] = await screen.findAllByLabelText('Vue affichée')
    await screen.findAllByRole('option', { name: 'Camille' })
    await user.selectOptions(switcher, camille.id)

    await waitFor(() => expect(pageText()).toContain('68 500 €'))
    expect(pageText()).toContain('Part détenue par Camille.')
    expect(seen).toContain(camille.id)
    expect(seen).toContain(`history:${camille.id}`)

    await user.selectOptions(switcher, '')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))
  })

  it('explique quoi faire quand il n’y a encore aucun compte', async () => {
    mockShell()
    mockJson('get', '/api/overview', emptyOverview)
    renderApp('/')
    expect(await screen.findByRole('heading', { name: 'Aucun compte pour l’instant' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Ajouter un compte' })).toHaveAttribute('href', '/comptes')
  })
})

describe('erreurs', () => {
  it('affiche « API injoignable » puis les données après un nouvel essai', async () => {
    mockShell()
    mockUnreachable('get', '/api/overview')
    const { user } = renderApp('/')
    expect(await screen.findByRole('alert')).toHaveTextContent('API injoignable')

    mockJson('get', '/api/overview', household)
    mockJson('get', '/api/history', { points: [] })
    await user.click(screen.getByRole('button', { name: 'Réessayer' }))
    await waitFor(() => expect(pageText()).toContain('120 000 €'))
  })

  it('affiche le message d’erreur renvoyé par l’API', async () => {
    mockShell()
    mockApiError('get', '/api/overview', 500, 'Calcul du patrimoine impossible.')
    renderApp('/')
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Chargement impossible')
    expect(alert).toHaveTextContent('Calcul du patrimoine impossible.')
  })

  it('efface le jeton et renvoie à la connexion sur une réponse 401', async () => {
    mockApiError('get', '/api/members', 401, 'Jeton expiré.')
    mockApiError('get', '/api/overview', 401, 'Jeton expiré.')
    mockJson('get', '/api/auth/status', { setup_required: false })
    renderApp('/')
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBeNull()
  })
})
