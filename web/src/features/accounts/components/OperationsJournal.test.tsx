import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Operation, OperationsSummary } from '@/shared/api/types'
import { buildAccountDetail, buildHolding } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const etf = buildHolding({ id: 'h-1', account_id: 'a-pea', name: 'Fonds Indice Monde' })

function buildOperation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: 'op-1',
    account_id: 'a-pea',
    kind: 'buy',
    date: '2025-01-10',
    holding_id: 'h-1',
    holding_name: 'Fonds Indice Monde',
    quantity: 10,
    unit_price: 100,
    amount: 1000,
    fees: 5,
    currency: 'EUR',
    note: null,
    realized_gain: null,
    ...overrides,
  }
}

const summary: OperationsSummary = {
  currency: 'EUR',
  years: [{ year: 2025, realized_gain: 272, dividends: 12.5, fees: 14, deposits: 3000, withdrawals: 0 }],
}

/** Compte PEA dont le journal est servi depuis un état modifiable. */
function servePea(initial: Operation[]) {
  const state = { operations: [...initial] }
  mockShell()
  mockJson(
    'get',
    '/api/accounts/a-pea',
    buildAccountDetail({ id: 'a-pea', name: 'PEA Camille', kind: 'investment', envelope: 'pea', holdings: [etf] }),
  )
  mockJson('get', '/api/accounts/a-pea/transactions', { items: [], total: 0 })
  mockJson('get', '/api/accounts/a-pea/operations/summary', summary)
  server.use(http.get(`${API}/api/accounts/a-pea/operations`, () => HttpResponse.json(state.operations)))
  return state
}

async function openJournal() {
  const view = renderApp('/comptes/a-pea')
  await screen.findByRole('heading', { name: 'PEA Camille' })
  await view.user.click(screen.getByRole('button', { name: 'Journal d’opérations' }))
  return view
}

describe('journal d’opérations d’un compte d’investissement', () => {
  it('liste les opérations et la synthèse annuelle', async () => {
    servePea([
      buildOperation({ id: 'op-2', kind: 'sell', date: '2025-06-01', quantity: 8, unit_price: 150, amount: 1200, fees: 4, realized_gain: 272 }),
      buildOperation(),
    ])
    await openJournal()

    const sale = await screen.findByRole('row', { name: /Vente Fonds Indice Monde/ })
    expect(pageText(sale)).toContain('+272,00 €')
    expect(pageText(sale)).toContain('+1 196,00 €')
    const buy = screen.getByRole('row', { name: /Achat Fonds Indice Monde/ })
    expect(pageText(buy)).toContain('-1 005,00 €')
    const year = within(await screen.findByRole('table', { name: 'Synthèse annuelle' })).getByRole('row', { name: /2025/ })
    expect(pageText(year)).toContain('+272,00 €')
    expect(pageText(year)).toContain('3 000,00 €')
  })

  it('affiche l’état vide sans synthèse', async () => {
    servePea([])
    await openJournal()
    expect(await screen.findByText(/Aucune opération\. Enregistrez vos achats/)).toBeInTheDocument()
    expect(screen.queryByRole('table', { name: 'Synthèse annuelle' })).not.toBeInTheDocument()
  })

  it('affiche l’erreur de chargement avec une relance', async () => {
    servePea([])
    mockApiError('get', '/api/accounts/a-pea/operations', 500, 'Erreur interne du serveur')
    await openJournal()
    expect(await screen.findByText('Erreur interne du serveur')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Réessayer/ })).toBeInTheDocument()
  })

  it('envoie un achat au format du contrat puis l’affiche', async () => {
    const state = servePea([])
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts/a-pea/operations`, async ({ request }) => {
        received = await request.json()
        const created = buildOperation({ id: 'op-new', quantity: 12.5, unit_price: 80, amount: 1000, fees: 2 })
        state.operations.push(created)
        return HttpResponse.json(created, { status: 201 })
      }),
    )
    const { user: actor } = await openJournal()
    await actor.click(await screen.findByRole('button', { name: 'Ajouter une opération' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter une opération' }))
    await actor.clear(dialog.getByLabelText('Date'))
    await actor.type(dialog.getByLabelText('Date'), '2025-01-10')
    await actor.selectOptions(dialog.getByLabelText('Position'), 'h-1')
    await actor.type(dialog.getByLabelText('Quantité'), '12,5')
    await actor.type(dialog.getByLabelText('Cours unitaire'), '80')
    await actor.type(dialog.getByLabelText('Frais'), '2')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({ kind: 'buy', date: '2025-01-10', holding_id: 'h-1', quantity: 12.5, unit_price: 80, fees: 2 })
    expect(await screen.findByRole('row', { name: /Achat Fonds Indice Monde/ })).toBeInTheDocument()
  })

  it('valide la saisie avant tout envoi et affiche le refus de l’API', async () => {
    servePea([])
    const calls = spyOn('post', '/api/accounts/a-pea/operations')
    const { user: actor } = await openJournal()
    await actor.click(await screen.findByRole('button', { name: 'Ajouter une opération' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter une opération' }))
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getByText('Choisissez la position achetée ou vendue.')).toBeInTheDocument()
    expect(dialog.getByText('Saisissez une quantité positive, par exemple 12,5.')).toBeInTheDocument()
    expect(calls.count).toBe(0)

    mockApiError('post', '/api/accounts/a-pea/operations', 400, 'Vente impossible le 2025-01-10 : seulement 0 titre(s) détenu(s) à cette date')
    await actor.selectOptions(dialog.getByLabelText('Type d’opération'), 'sell')
    await actor.selectOptions(dialog.getByLabelText('Position'), 'h-1')
    await actor.type(dialog.getByLabelText('Quantité'), '1')
    await actor.type(dialog.getByLabelText('Cours unitaire'), '10')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(await dialog.findByText(/Vente impossible le 2025-01-10/)).toBeInTheDocument()
  })

  it('n’affiche que le montant pour un versement', async () => {
    servePea([])
    const { user: actor } = await openJournal()
    await actor.click(await screen.findByRole('button', { name: 'Ajouter une opération' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter une opération' }))
    await actor.selectOptions(dialog.getByLabelText('Type d’opération'), 'deposit')
    expect(dialog.getByLabelText('Montant')).toBeInTheDocument()
    expect(dialog.queryByLabelText('Position')).not.toBeInTheDocument()
    expect(dialog.queryByLabelText('Quantité')).not.toBeInTheDocument()
  })

  it('supprime une opération après confirmation', async () => {
    const state = servePea([buildOperation()])
    server.use(
      http.delete(`${API}/api/operations/op-1`, () => {
        state.operations = []
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const { user: actor } = await openJournal()
    await actor.click(await screen.findByRole('button', { name: /Supprimer l’opération Achat Fonds Indice Monde/ }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Supprimer cette opération ?' }))
    await actor.click(dialog.getByRole('button', { name: 'Supprimer l’opération' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(await screen.findByText(/Aucune opération\. Enregistrez vos achats/)).toBeInTheDocument()
  })
})
