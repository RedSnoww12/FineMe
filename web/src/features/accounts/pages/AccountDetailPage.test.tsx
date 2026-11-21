import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Holding, Transaction } from '@/shared/api/types'
import { buildAccountDetail, buildHolding } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

function mockInvestmentAccount(holdings: ReturnType<typeof buildHolding>[]) {
  mockShell()
  mockJson(
    'get',
    '/api/accounts/a-pea',
    buildAccountDetail({ id: 'a-pea', name: 'PEA Camille', kind: 'investment', envelope: 'pea', holdings }),
  )
  mockJson('get', '/api/accounts/a-pea/transactions', { items: [], total: 0 })
}

describe('positions d’un compte', () => {
  it('signale les cours automatiques qui ne sont plus mis à jour', async () => {
    mockInvestmentAccount([
      buildHolding(),
      buildHolding({ id: 'h-2', name: 'Action Démo', symbol: 'DEMO', price_stale: true }),
    ])
    renderApp('/comptes/a-pea')

    const stale = await screen.findByRole('row', { name: /Action Démo/ })
    expect(within(stale).getByText('ancien')).toBeInTheDocument()
    const fresh = screen.getByRole('row', { name: /Fonds Indice Monde/ })
    expect(within(fresh).queryByText('ancien')).not.toBeInTheDocument()
    expect(
      screen.getByText(/Un cours n’est plus mis à jour automatiquement depuis plusieurs jours/),
    ).toBeInTheDocument()
  })

  it('n’affiche aucun avertissement quand les cours sont à jour ou saisis', async () => {
    mockInvestmentAccount([
      buildHolding(),
      buildHolding({ id: 'h-2', name: 'Fonds euro', kind: 'fonds_euro', price_source: 'manual' }),
    ])
    renderApp('/comptes/a-pea')

    const manual = await screen.findByRole('row', { name: /Fonds euro/ })
    expect(within(manual).getByText('saisi')).toBeInTheDocument()
    expect(screen.queryByText('ancien')).not.toBeInTheDocument()
    expect(screen.queryByText(/plus mis à jour automatiquement/)).not.toBeInTheDocument()
  })
})

/** Compte d'investissement servi depuis un état modifiable : on voit l'effet des écritures au rechargement. */
function serveInvestmentAccount(initial: Holding[]) {
  const state = { holdings: [...initial] }
  mockShell()
  server.use(
    http.get(`${API}/api/accounts/a-pea`, () =>
      HttpResponse.json(
        buildAccountDetail({ id: 'a-pea', name: 'PEA Camille', kind: 'investment', envelope: 'pea', holdings: state.holdings }),
      ),
    ),
  )
  mockJson('get', '/api/accounts/a-pea/transactions', { items: [], total: 0 })
  return state
}

async function openPea() {
  const view = renderApp('/comptes/a-pea')
  await screen.findByRole('heading', { name: 'PEA Camille' })
  return view
}

async function openHoldingForm(actor: ReturnType<typeof renderApp>['user']) {
  await actor.click(screen.getByRole('button', { name: 'Ajouter une position' }))
  return within(await screen.findByRole('dialog', { name: 'Ajouter une position' }))
}

describe('ajout, modification et suppression d’une position', () => {
  it('envoie la nouvelle position au format du contrat puis l’affiche', async () => {
    const state = serveInvestmentAccount([])
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts/a-pea/holdings`, async ({ request }) => {
        received = await request.json()
        const created = buildHolding({ id: 'h-new', account_id: 'a-pea', name: 'Fonds Obligataire Test', symbol: 'FOT' })
        state.holdings.push(created)
        return HttpResponse.json(created)
      }),
    )
    const { user: actor } = await openPea()
    expect(screen.getByText(/Aucune position/)).toBeInTheDocument()

    const dialog = await openHoldingForm(actor)
    await actor.type(dialog.getByLabelText('Nom'), 'Fonds Obligataire Test')
    await actor.type(dialog.getByLabelText('Code ISIN'), 'fr0000000002')
    await actor.type(dialog.getByLabelText('Symbole'), 'fot')
    await actor.type(dialog.getByLabelText('Quantité'), '12,5')
    await actor.type(dialog.getByLabelText('Prix unitaire actuel'), '104,2')
    await actor.type(dialog.getByLabelText('Prix de revient unitaire'), '100')
    await actor.type(dialog.getByLabelText('Frais courants annuels'), '0,25')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({
      kind: 'security',
      name: 'Fonds Obligataire Test',
      isin: 'FR0000000002',
      symbol: 'FOT',
      security_type: null,
      quantity: 12.5,
      buy_price: 100,
      current_price: 104.2,
      currency: 'EUR',
      expense_ratio: 0.0025,
    })
    expect(await screen.findByRole('row', { name: /Fonds Obligataire Test/ })).toBeInTheDocument()
  })

  it('valide la saisie avant tout envoi', async () => {
    serveInvestmentAccount([])
    const calls = spyOn('post', '/api/accounts/a-pea/holdings')
    const { user: actor } = await openPea()
    const dialog = await openHoldingForm(actor)
    await actor.type(dialog.getByLabelText('Code ISIN'), 'FR123')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    expect(dialog.getByText('Donnez un nom à cette position.')).toBeInTheDocument()
    expect(dialog.getByText('Saisissez une quantité, par exemple 12,5.')).toBeInTheDocument()
    expect(dialog.getByText('Saisissez le prix unitaire actuel.')).toBeInTheDocument()
    expect(dialog.getByText('Un code ISIN compte 12 caractères, par exemple FR0011869353.')).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('affiche le refus de l’API sans fermer le formulaire', async () => {
    serveInvestmentAccount([])
    mockApiError('post', '/api/accounts/a-pea/holdings', 422, 'La quantité doit être positive.')
    const { user: actor } = await openPea()
    const dialog = await openHoldingForm(actor)
    await actor.type(dialog.getByLabelText('Nom'), 'Fonds Obligataire Test')
    await actor.type(dialog.getByLabelText('Quantité'), '1')
    await actor.type(dialog.getByLabelText('Prix unitaire actuel'), '10')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    expect(await dialog.findByText('La quantité doit être positive.')).toBeInTheDocument()
    expect(screen.getByRole('dialog', { name: 'Ajouter une position' })).toBeInTheDocument()
  })

  it('préremplit la position modifiée et envoie les nouvelles valeurs', async () => {
    const state = serveInvestmentAccount([buildHolding({ account_id: 'a-pea' })])
    let received: unknown
    server.use(
      http.put(`${API}/api/holdings/h-1`, async ({ request }) => {
        received = await request.json()
        const updated = buildHolding({ account_id: 'a-pea', quantity: 120, value: 39_600 })
        state.holdings = [updated]
        return HttpResponse.json(updated)
      }),
    )
    const { user: actor } = await openPea()
    await actor.click(screen.getByRole('button', { name: 'Modifier Fonds Indice Monde' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Modifier la position' }))
    expect(dialog.getByLabelText('Nom')).toHaveValue('Fonds Indice Monde')
    expect(dialog.getByLabelText('Code ISIN')).toHaveValue('FR0000000001')
    expect(dialog.getByLabelText('Frais courants annuels')).toHaveValue('0,2')

    await actor.clear(dialog.getByLabelText('Quantité'))
    await actor.type(dialog.getByLabelText('Quantité'), '120')
    await actor.click(dialog.getByRole('button', { name: 'Enregistrer' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toMatchObject({ name: 'Fonds Indice Monde', quantity: 120, current_price: 330, expense_ratio: 0.002 })
    await waitFor(() => expect(pageText(screen.getByRole('row', { name: /Fonds Indice Monde/ }))).toContain('39 600 €'))
  })

  it('supprime une position après confirmation', async () => {
    const state = serveInvestmentAccount([buildHolding({ account_id: 'a-pea' })])
    let deleted = false
    server.use(
      http.delete(`${API}/api/holdings/h-1`, () => {
        deleted = true
        state.holdings = []
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const { user: actor } = await openPea()
    await actor.click(screen.getByRole('button', { name: 'Supprimer Fonds Indice Monde' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Supprimer cette position ?' }))
    await actor.click(dialog.getByRole('button', { name: 'Supprimer la position' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(deleted).toBe(true)
    expect(await screen.findByText(/Aucune position/)).toBeInTheDocument()
  })

  it('garde la confirmation ouverte avec le message de l’API quand la suppression échoue', async () => {
    serveInvestmentAccount([buildHolding({ account_id: 'a-pea' })])
    mockApiError('delete', '/api/holdings/h-1', 404, 'Position introuvable.')
    const { user: actor } = await openPea()
    await actor.click(screen.getByRole('button', { name: 'Supprimer Fonds Indice Monde' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Supprimer cette position ?' }))
    await actor.click(dialog.getByRole('button', { name: 'Supprimer la position' }))

    expect(await dialog.findByText('Position introuvable.')).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /Fonds Indice Monde/ })).toBeInTheDocument()
  })
})

describe('actualisation des cours', () => {
  it('résume les cours mis à jour et les positions sans cotation', async () => {
    serveInvestmentAccount([
      buildHolding({ account_id: 'a-pea' }),
      buildHolding({ id: 'h-2', account_id: 'a-pea', name: 'Action Démo', symbol: 'DEMO' }),
    ])
    let calls = 0
    server.use(
      http.post(`${API}/api/prices/refresh`, () => {
        calls += 1
        return HttpResponse.json({
          updated: 1,
          failed: [{ holding_id: 'h-2', name: 'Action Démo', reason: 'aucune cotation trouvée' }],
        })
      }),
    )
    const { user: actor } = await openPea()
    await actor.click(screen.getByRole('button', { name: 'Actualiser les cours' }))

    const notice = await screen.findByText(/1 cours mis à jour\./)
    expect(notice).toHaveTextContent(
      'Sans cotation trouvée, ces positions gardent leur prix saisi : Action Démo (aucune cotation trouvée).',
    )
    expect(calls).toBe(1)
  })

  it('affiche l’erreur de l’API', async () => {
    serveInvestmentAccount([buildHolding({ account_id: 'a-pea' })])
    mockApiError('post', '/api/prices/refresh', 502, 'Fournisseur de cours indisponible.')
    const { user: actor } = await openPea()
    await actor.click(screen.getByRole('button', { name: 'Actualiser les cours' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Fournisseur de cours indisponible.')
  })

  it('n’est pas proposée sans position', async () => {
    serveInvestmentAccount([])
    await openPea()
    expect(screen.queryByRole('button', { name: 'Actualiser les cours' })).not.toBeInTheDocument()
  })
})

function buildTransaction(overrides: Partial<Transaction> = {}): Transaction {
  return {
    id: 't-1',
    account_id: 'a-1',
    date: '2025-02-03',
    label: 'Boulangerie du Port',
    amount: -4.2,
    category: 'Alimentation',
    manual_category: null,
    internal_transfer: null,
    ...overrides,
  }
}

async function openCheckingAccount() {
  mockShell()
  mockJson('get', '/api/accounts/a-1', buildAccountDetail())
  const view = renderApp('/comptes/a-1')
  await screen.findByRole('heading', { name: 'Compte joint' })
  return view
}

async function transactionsPanel() {
  const heading = await screen.findByRole('heading', { name: 'Opérations' })
  const panel = heading.closest('section')
  if (!panel) throw new Error('Panneau des opérations introuvable')
  return panel
}

describe('opérations d’un compte', () => {
  it('liste les opérations avec date, catégorie et montant signé', async () => {
    mockJson('get', '/api/accounts/a-1/transactions', {
      items: [
        buildTransaction(),
        buildTransaction({ id: 't-2', date: '2025-02-05', label: 'Virement salaire', amount: 2100, category: null }),
      ],
      total: 2,
    })
    await openCheckingAccount()
    const panel = await transactionsPanel()
    await within(panel).findByText('Boulangerie du Port')

    const text = pageText(panel)
    expect(text).toContain('03/02/2025, Alimentation')
    expect(text).toContain('-4,20 €')
    expect(text).toContain('+2 100,00 €')
    // Moins d'une page : pas de pagination.
    expect(within(panel).queryByRole('button', { name: 'Suivantes' })).not.toBeInTheDocument()
  })

  it('pagine par 25 en demandant la page suivante à l’API', async () => {
    const pages: string[] = []
    server.use(
      http.get(`${API}/api/accounts/a-1/transactions`, ({ request }) => {
        const url = new URL(request.url)
        const offset = Number(url.searchParams.get('offset'))
        pages.push(`${url.searchParams.get('limit')}:${offset}`)
        const items = Array.from({ length: offset === 0 ? 25 : 5 }, (_, i) =>
          buildTransaction({ id: `t-${offset + i}`, label: `Opération ${offset + i + 1}` }),
        )
        return HttpResponse.json({ items, total: 30 })
      }),
    )
    const { user: actor } = await openCheckingAccount()
    const panel = await transactionsPanel()
    await within(panel).findByText('Opération 1')
    expect(pageText(panel)).toContain('1 à 25 sur 30')

    await actor.click(within(panel).getByRole('button', { name: 'Suivantes' }))
    expect(await within(panel).findByText('Opération 26')).toBeInTheDocument()
    expect(pageText(panel)).toContain('26 à 30 sur 30')
    expect(within(panel).getByRole('button', { name: 'Suivantes' })).toBeDisabled()
    expect(pages).toEqual(['25:0', '25:25'])
  })

  it('supprime une opération après confirmation', async () => {
    const state = { items: [buildTransaction()] }
    let deleted = false
    server.use(
      http.get(`${API}/api/accounts/a-1/transactions`, () => HttpResponse.json({ items: state.items, total: state.items.length })),
      http.delete(`${API}/api/transactions/t-1`, () => {
        deleted = true
        state.items = []
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const { user: actor } = await openCheckingAccount()
    await actor.click(await screen.findByRole('button', { name: 'Supprimer l’opération Boulangerie du Port' }))
    const dialog = await screen.findByRole('dialog', { name: 'Supprimer cette opération ?' })
    expect(pageText(dialog)).toContain('« Boulangerie du Port » du 03/02/2025 sera supprimée.')
    await actor.click(within(dialog).getByRole('button', { name: 'Supprimer l’opération' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(deleted).toBe(true)
    expect(await screen.findByText(/Aucune opération/)).toBeInTheDocument()
  })

  it('affiche l’état d’erreur des opérations sans masquer le compte', async () => {
    mockApiError('get', '/api/accounts/a-1/transactions', 500, 'Lecture des opérations impossible.')
    await openCheckingAccount()
    const panel = await transactionsPanel()
    expect(await within(panel).findByRole('alert')).toHaveTextContent('Lecture des opérations impossible.')
    expect(within(panel).getByRole('button', { name: 'Réessayer' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Compte joint' })).toBeInTheDocument()
  })
})
