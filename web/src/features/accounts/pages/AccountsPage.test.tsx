import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Account } from '@/shared/api/types'
import { buildAccount, buildAccountDetail, camille } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

async function openCreateForm() {
  mockShell()
  const view = renderApp('/comptes')
  await view.user.click(await screen.findByRole('button', { name: 'Ajouter un compte' }))
  const dialog = await screen.findByRole('dialog', { name: 'Ajouter un compte' })
  await within(dialog).findByLabelText('Nom du compte')
  return { ...view, dialog: within(dialog) }
}

describe('liste des comptes', () => {
  it('explique comment commencer quand la liste est vide', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [])
    renderApp('/comptes')
    expect(await screen.findByRole('heading', { name: 'Aucun compte' })).toBeInTheDocument()
  })

  it('regroupe les comptes par catégorie avec propriétaires et parts', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [
      buildAccount(),
      buildAccount({
        id: 'a-2',
        name: 'PEA Camille',
        kind: 'investment',
        envelope: 'pea',
        value: 33_000,
        owned_value: 33_000,
        gain: 3_000,
        invested: 30_000,
        owners: [{ member_id: camille.id, name: 'Camille', share: 1 }],
      }),
    ])
    renderApp('/comptes')
    expect(await screen.findByRole('heading', { name: 'Comptes courants' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Investissements' })).toBeInTheDocument()
    const text = pageText()
    expect(text).toContain('Banque du Canal, Camille 50 %, Sacha 50 %')
    expect(text).toContain('33 000 €')
    expect(text).toContain('+3 000 €')
    expect(screen.getByText('PEA')).toBeInTheDocument()
  })
})

describe('création d’un compte', () => {
  it('refuse un formulaire incomplet ou des parts incohérentes sans appeler l’API', async () => {
    mockJson('get', '/api/accounts', [])
    const createCalls = spyOn('post', '/api/accounts')
    const { user, dialog } = await openCreateForm()

    await user.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getByText('Donnez un nom à ce compte.')).toBeInTheDocument()

    await user.type(dialog.getByLabelText('Nom du compte'), 'Livret Horizon')
    await user.type(dialog.getByLabelText('Solde espèces'), 'beaucoup')
    await user.clear(dialog.getByLabelText('Part'))
    await user.type(dialog.getByLabelText('Part'), '150')
    await user.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getByText('Saisissez un nombre, par exemple 1 234,56.')).toBeInTheDocument()
    expect(dialog.getByText('Chaque part doit être comprise entre 0 et 100 %.')).toBeInTheDocument()
    expect(createCalls.count).toBe(0)
  })

  it('refuse des parts dont le total dépasse 100 %', async () => {
    mockJson('get', '/api/accounts', [])
    const createCalls = spyOn('post', '/api/accounts')
    const { user, dialog } = await openCreateForm()
    await user.type(dialog.getByLabelText('Nom du compte'), 'Compte joint')
    await user.clear(dialog.getByLabelText('Part'))
    await user.type(dialog.getByLabelText('Part'), '60')
    await user.click(dialog.getByRole('button', { name: 'Ajouter un propriétaire' }))
    const shares = dialog.getAllByLabelText('Part')
    await user.clear(shares[1])
    await user.type(shares[1], '50')
    expect(pageText()).toContain('Total : 110 %')
    await user.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getByText('Le total des parts ne peut pas dépasser 100 %.')).toBeInTheDocument()
    expect(createCalls.count).toBe(0)
  })

  it('envoie le compte au format du contrat puis l’affiche dans la liste', async () => {
    const stored: Account[] = []
    let received: unknown
    server.use(
      http.get(`${API}/api/accounts`, () => HttpResponse.json(stored)),
      http.post(`${API}/api/accounts`, async ({ request }) => {
        received = await request.json()
        const created = buildAccountDetail({
          id: 'a-nouveau',
          name: 'Livret Horizon',
          kind: 'savings',
          envelope: 'livret',
          institution: 'Caisse des Collines',
          cash_balance: 1234.56,
          value: 1234.56,
          owned_value: 1234.56,
          annual_yield_pct: 2.4,
          owners: [{ member_id: camille.id, name: 'Camille', share: 1 }],
        })
        stored.push(created)
        return HttpResponse.json(created)
      }),
    )
    const { user, dialog } = await openCreateForm()

    await user.selectOptions(dialog.getByLabelText('Type de compte'), 'savings')
    await user.type(dialog.getByLabelText('Nom du compte'), 'Livret Horizon')
    await user.type(dialog.getByLabelText('Établissement'), 'Caisse des Collines')
    await user.selectOptions(dialog.getByLabelText('Enveloppe fiscale'), 'livret')
    await user.type(dialog.getByLabelText('Solde espèces'), '1 234,56')
    await user.type(dialog.getByLabelText('Rendement annuel'), '2,4')
    await user.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({
      name: 'Livret Horizon',
      kind: 'savings',
      owners: [{ member_id: camille.id, share: 1 }],
      institution: 'Caisse des Collines',
      envelope: 'livret',
      currency: 'EUR',
      cash_balance: 1234.56,
      annual_yield_pct: 2.4,
      opened_at: null,
    })
    expect(await screen.findByText('Livret Horizon')).toBeInTheDocument()
    expect(pageText()).toContain('1 235 €')
  })

  it('affiche le message de l’API quand l’enregistrement échoue', async () => {
    mockJson('get', '/api/accounts', [])
    mockApiError('post', '/api/accounts', 422, 'Un compte porte déjà ce nom.')
    const { user, dialog } = await openCreateForm()
    await user.type(dialog.getByLabelText('Nom du compte'), 'Compte joint')
    await user.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(await dialog.findByText('Un compte porte déjà ce nom.')).toBeInTheDocument()
    expect(screen.getByRole('dialog')).toBeInTheDocument()
  })
})
