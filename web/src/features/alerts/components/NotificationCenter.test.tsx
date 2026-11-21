import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Alert } from '@/shared/api/types'
import { emptyOverview } from '@/test/fixtures'
import { mockOverviewScreen, mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

function buildAlert(overrides: Partial<Alert> = {}): Alert {
  return {
    id: 'al-1',
    kind: 'savings_ceiling',
    severity: 'warning',
    title: 'Plafond proche : Livret Fictif',
    message: '« Livret Fictif » atteint 95 % de son plafond (9 500 € sur 10 000 €).',
    created_at: '2026-03-02T07:00:00Z',
    read: false,
    archived: false,
    active: true,
    ...overrides,
  }
}

/** API simulée avec un état : les mises à jour modifient la liste relue ensuite. */
function mockAlertsState(initial: Alert[]) {
  let alerts = initial
  const updates: unknown[] = []
  server.use(
    http.get(`${API}/api/alerts`, () => HttpResponse.json(alerts.filter((alert) => !alert.archived))),
    http.put(`${API}/api/alerts/:id`, async ({ params, request }) => {
      const body = (await request.json()) as Partial<Alert>
      updates.push({ id: params.id, ...body })
      alerts = alerts.map((alert) => (alert.id === params.id ? { ...alert, ...body } : alert))
      return HttpResponse.json(alerts.find((alert) => alert.id === params.id))
    }),
    http.post(`${API}/api/alerts/read-all`, () => {
      const updated = alerts.filter((alert) => !alert.read).length
      alerts = alerts.map((alert) => ({ ...alert, read: true }))
      return HttpResponse.json({ updated })
    }),
  )
  return updates
}

/** Zone aria-live du shell qui annonce le nombre de non lues. */
function announcer(): HTMLElement {
  const region = screen.getAllByRole('status').find((element) => /notification/.test(element.textContent ?? ''))
  if (!region) throw new Error('Annonce des notifications introuvable')
  return region
}

async function openCenter(actor: ReturnType<typeof renderApp>['user']) {
  const [bell] = await screen.findAllByRole('button', { name: /^Notifications/ })
  await actor.click(bell)
  return screen.findByRole('dialog', { name: 'Notifications' })
}

describe('centre de notifications', () => {
  it('annonce le nombre de non lues et l’affiche sur la cloche', async () => {
    mockShell()
    mockOverviewScreen(emptyOverview)
    mockAlertsState([buildAlert(), buildAlert({ id: 'al-2', title: 'Objectif atteint : Voyage', read: true })])
    renderApp('/')

    await waitFor(() => expect(announcer()).toHaveTextContent('1 notification non lue'))
    const bells = screen.getAllByRole('button', { name: 'Notifications (1 notification non lue)' })
    expect(bells.length).toBeGreaterThan(0)
    expect(bells[0]).toHaveTextContent('1')
  })

  it('liste les alertes, les marque comme lues et les archive', async () => {
    mockShell()
    mockOverviewScreen(emptyOverview)
    const updates = mockAlertsState([
      buildAlert(),
      buildAlert({ id: 'al-2', kind: 'stale_prices', severity: 'info', title: '1 cours n’est plus mis à jour', message: 'Cours à vérifier : ETF Démo.' }),
    ])
    const { user: actor } = renderApp('/')
    const dialog = await openCenter(actor)

    const list = await within(dialog).findByRole('list', { name: 'Notifications' })
    expect(within(list).getAllByRole('listitem')).toHaveLength(2)
    expect(within(dialog).getByText('2 notifications non lues.')).toBeInTheDocument()
    expect(within(dialog).getByText(/9 500/)).toBeInTheDocument()

    await actor.click(within(dialog).getByRole('button', { name: 'Marquer comme lue : Plafond proche : Livret Fictif' }))
    await waitFor(() => expect(announcer()).toHaveTextContent('1 notification non lue'))
    expect(updates).toContainEqual({ id: 'al-1', read: true })

    await actor.click(within(dialog).getByRole('button', { name: 'Archiver : 1 cours n’est plus mis à jour' }))
    await waitFor(() => expect(within(dialog).getAllByRole('listitem')).toHaveLength(1))
    expect(updates).toContainEqual({ id: 'al-2', archived: true, read: true })
    expect(announcer()).toHaveTextContent('Aucune notification non lue')
  })

  it('marque tout comme lu et se ferme au clavier', async () => {
    mockShell()
    mockOverviewScreen(emptyOverview)
    mockAlertsState([buildAlert(), buildAlert({ id: 'al-2', title: 'Connexion expirée : Banque Fictive' })])
    const { user: actor } = renderApp('/')
    const dialog = await openCenter(actor)

    await actor.click(await within(dialog).findByRole('button', { name: 'Tout marquer comme lu' }))
    await waitFor(() => expect(announcer()).toHaveTextContent('Aucune notification non lue'))
    expect(within(dialog).getByRole('button', { name: 'Tout marquer comme lu' })).toBeDisabled()

    await actor.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('gère le vide, l’erreur et la vérification à la demande', async () => {
    mockShell()
    mockOverviewScreen(emptyOverview)
    let evaluated = 0
    server.use(
      http.post(`${API}/api/alerts/evaluate`, () => {
        evaluated += 1
        return HttpResponse.json({ created: 0, reopened: 0, resolved: 0 })
      }),
    )
    const { user: actor } = renderApp('/')
    const dialog = await openCenter(actor)
    expect(await within(dialog).findByText('Aucune notification')).toBeInTheDocument()
    await actor.click(within(dialog).getByRole('button', { name: 'Vérifier maintenant' }))
    await waitFor(() => expect(evaluated).toBe(1))

    mockApiError('get', '/api/alerts', 500, 'Service indisponible')
    mockApiError('post', '/api/alerts/evaluate', 500, 'Évaluation impossible')
    await actor.click(within(dialog).getByRole('button', { name: 'Vérifier maintenant' }))
    expect(await within(dialog).findByText('Évaluation impossible')).toBeInTheDocument()
  })

  it('n’affiche pas les montants en mode discret', async () => {
    mockShell()
    mockOverviewScreen(emptyOverview)
    mockJson('get', '/api/alerts', [buildAlert()])
    const { user: actor } = renderApp('/')
    await screen.findAllByRole('button', { name: /1 notification non lue/ })
    const [discreet] = screen.getAllByRole('button', { name: 'Masquer les montants' })
    await actor.click(discreet)
    const dialog = await openCenter(actor)
    expect(within(dialog).queryByText(/9 500/)).not.toBeInTheDocument()
    expect(within(dialog).getByText(/Montants masqués/)).toBeInTheDocument()
  })
})
