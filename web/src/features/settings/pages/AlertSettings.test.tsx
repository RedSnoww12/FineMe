import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { AlertRule } from '@/shared/api/types'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const RULES: AlertRule[] = [
  { kind: 'goals', enabled: true, threshold_pct: null, ceiling: null },
  { kind: 'savings_ceiling', enabled: true, threshold_pct: 90, ceiling: 10_000 },
  { kind: 'net_worth_change', enabled: true, threshold_pct: 10, ceiling: null },
  { kind: 'connections', enabled: true, threshold_pct: null, ceiling: null },
  { kind: 'stale_prices', enabled: true, threshold_pct: null, ceiling: null },
]

/** Règles avec un état : PUT modifie ce que relit GET. */
function mockRules() {
  let rules = RULES
  const updates: unknown[] = []
  server.use(
    http.get(`${API}/api/alerts/rules`, () => HttpResponse.json(rules)),
    http.put(`${API}/api/alerts/rules/:kind`, async ({ params, request }) => {
      const body = (await request.json()) as Partial<AlertRule>
      updates.push({ kind: params.kind, ...body })
      rules = rules.map((rule) => (rule.kind === params.kind ? { ...rule, ...body } : rule))
      return HttpResponse.json(rules.find((rule) => rule.kind === params.kind))
    }),
  )
  return updates
}

async function openAlertSettings() {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  const view = renderApp('/parametres')
  const panel = (await screen.findByRole('heading', { name: 'Alertes et rappels' })).closest('section') as HTMLElement
  await within(panel).findByRole('list', { name: 'Règles d’alerte' })
  return { ...view, panel }
}

describe('réglage des alertes dans Paramètres', () => {
  it('désactive puis réactive une alerte', async () => {
    const updates = mockRules()
    const { user: actor, panel } = await openAlertSettings()

    const ceiling = within(panel).getByRole('checkbox', { name: /Plafond des livrets/ })
    expect(ceiling).toBeChecked()
    expect(within(panel).getByLabelText('Plafond des livrets', { selector: 'input:not([type=checkbox])' })).toHaveValue('10000')

    await actor.click(ceiling)
    await waitFor(() => expect(within(panel).getByRole('checkbox', { name: /Plafond des livrets/ })).not.toBeChecked())
    expect(updates).toEqual([{ kind: 'savings_ceiling', enabled: false }])
    // Désactivée, la règle n'affiche plus ses seuils.
    expect(within(panel).queryByRole('form', { name: 'Seuils : Plafond des livrets' })).not.toBeInTheDocument()

    await actor.click(within(panel).getByRole('checkbox', { name: /Plafond des livrets/ }))
    await waitFor(() => expect(within(panel).getByRole('checkbox', { name: /Plafond des livrets/ })).toBeChecked())
  })

  it('enregistre les seuils saisis au format français', async () => {
    const updates = mockRules()
    const { user: actor, panel } = await openAlertSettings()
    const form = within(panel).getByRole('form', { name: 'Seuils : Plafond des livrets' })

    const threshold = within(form).getByLabelText('Seuil (en % du plafond)')
    await actor.clear(threshold)
    await actor.type(threshold, '85,5')
    const amount = within(form).getByLabelText('Plafond des livrets')
    await actor.clear(amount)
    await actor.type(amount, '12 000')
    await actor.click(within(form).getByRole('button', { name: 'Enregistrer les seuils' }))

    expect(await within(form).findByText('Seuils enregistrés.')).toBeInTheDocument()
    expect(updates).toEqual([{ kind: 'savings_ceiling', threshold_pct: 85.5, ceiling: 12_000 }])
  })

  it('refuse un seuil invalide sans rien envoyer', async () => {
    mockRules()
    const calls = spyOn('put', '/api/alerts/rules/:kind')
    const { user: actor, panel } = await openAlertSettings()
    const form = within(panel).getByRole('form', { name: 'Seuils : Variation du patrimoine' })
    const threshold = within(form).getByLabelText('Seuil de variation')
    await actor.clear(threshold)
    await actor.type(threshold, '0')
    await actor.click(within(form).getByRole('button', { name: 'Enregistrer les seuils' }))
    expect(within(form).getByText(/supérieur à 0 et au plus 1\s000/)).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('affiche l’erreur de chargement', async () => {
    mockApiError('get', '/api/alerts/rules', 500, 'Service indisponible')
    mockShell()
    mockJson('get', '/api/auth/logins', [])
    renderApp('/parametres')
    const panel = (await screen.findByRole('heading', { name: 'Alertes et rappels' })).closest('section') as HTMLElement
    expect(await within(panel).findByText('Service indisponible')).toBeInTheDocument()
  })
})
