import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Account } from '@/shared/api/types'
import {
  buildAccount,
  buildRealEstate,
  buildRealEstateItem,
  buildRealEstateLoan,
  buildScheduleRows,
  camille,
} from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const house = buildRealEstateItem()
const houseAccount = house.account
const loanAccount: Account = buildAccount({
  id: 'a-pret',
  name: 'Prêt maison',
  kind: 'loan',
  value: 600,
  owned_value: 600,
  cash_balance: 0,
  is_liability: true,
})

function mockRealEstate(accounts: Account[] = [houseAccount, loanAccount]) {
  mockShell()
  mockJson('get', '/api/real-estate', buildRealEstate([house]))
  mockJson('get', '/api/accounts', accounts)
}

async function openRealEstate() {
  const view = renderApp('/immobilier')
  await screen.findByRole('heading', { name: 'Maison' })
  return view
}

function propertyCard(name: string) {
  const card = screen.getByRole('heading', { name }).closest('section')
  if (!card) throw new Error(`Fiche du bien ${name} introuvable`)
  return card
}

function panel(title: string) {
  const section = screen.getByRole('heading', { name: title }).closest('section')
  if (!section) throw new Error(`Panneau ${title} introuvable`)
  return section
}

describe('biens immobiliers', () => {
  it('présente les totaux, le bien et son prêt lié', async () => {
    mockRealEstate()
    await openRealEstate()

    const text = pageText()
    expect(text).toContain('Valeur des biens2 000 €')
    expect(text).toContain('Capital restant dû600 €')
    expect(text).toContain('Valeur nette1 400 €')

    const card = propertyCard('Maison')
    const cardText = pageText(card)
    expect(cardText).toContain('Résidence principale')
    expect(cardText).toContain('acheté 1 800 €')
    expect(cardText).toContain('+100 €')
    expect(cardText).toContain('Non loué')
    expect(within(card).getByRole('heading', { name: 'Prêt lié' })).toBeInTheDocument()
    expect(cardText).toContain('Prêt maison')
    expect(cardText).toContain('100,00 € par mois, fin déc. 2026')
    // Le prêt est lié au bien : il n'apparaît pas parmi les prêts sans bien.
    expect(screen.queryByRole('heading', { name: 'Prêts sans bien lié' })).not.toBeInTheDocument()
  })

  it('liste à part un prêt qui ne finance aucun bien', async () => {
    mockRealEstate([houseAccount, loanAccount, buildAccount({ id: 'a-auto', name: 'Crédit auto', kind: 'loan', owned_value: 8_000, is_liability: true })])
    await openRealEstate()
    expect(pageText(panel('Prêts sans bien lié'))).toContain('Crédit auto')
    expect(pageText(panel('Prêts sans bien lié'))).toContain('8 000 €')
  })

  it('invite à ajouter un premier bien', async () => {
    mockShell()
    mockJson('get', '/api/real-estate', buildRealEstate([]))
    mockJson('get', '/api/accounts', [])
    renderApp('/immobilier')
    expect(await screen.findByRole('heading', { name: 'Aucun bien immobilier' })).toBeInTheDocument()
  })

  it('affiche l’état d’erreur quand l’API échoue', async () => {
    mockShell()
    mockApiError('get', '/api/real-estate', 500, 'Lecture des biens impossible.')
    mockJson('get', '/api/accounts', [])
    renderApp('/immobilier')
    expect(await screen.findByRole('alert')).toHaveTextContent('Lecture des biens impossible.')
    expect(screen.queryByRole('button', { name: 'Ajouter un bien' })).not.toBeInTheDocument()
  })
})

describe('ajout d’un bien et d’un prêt lié', () => {
  it('envoie le bien au format du contrat', async () => {
    mockRealEstate()
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ ...houseAccount, id: 'a-studio', name: 'Studio', holdings: [], property: house.property, loan: null })
      }),
    )
    const { user: actor } = await openRealEstate()
    await actor.click(screen.getByRole('button', { name: 'Ajouter un bien' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un bien' }))
    await actor.type(await dialog.findByLabelText('Nom du bien'), 'Studio')
    await actor.selectOptions(dialog.getByLabelText('Usage'), 'rental')
    await actor.type(dialog.getByLabelText('Valeur actuelle estimée'), '150 000')
    await actor.type(dialog.getByLabelText('Prix d’achat'), '140 000')
    await actor.type(dialog.getByLabelText('Frais d’acquisition'), '10 500')
    await actor.type(dialog.getByLabelText('Surface'), '28,5')
    await actor.type(dialog.getByLabelText('Loyer mensuel'), '650')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({
      name: 'Studio',
      kind: 'real_estate',
      owners: [{ member_id: camille.id, share: 1 }],
      currency: 'EUR',
      property: {
        address: null,
        category: 'rental',
        building_type: 'apartment',
        surface_m2: 28.5,
        purchase_price: 140_000,
        purchase_fees: 10_500,
        purchased_at: null,
        current_value: 150_000,
        monthly_rent: 650,
        monthly_charges: 0,
        yearly_property_tax: 0,
      },
    })
  })

  it('exige la valeur et le prix d’achat avant tout envoi', async () => {
    mockRealEstate()
    const calls = spyOn('post', '/api/accounts')
    const { user: actor } = await openRealEstate()
    await actor.click(screen.getByRole('button', { name: 'Ajouter un bien' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un bien' }))
    await actor.type(await dialog.findByLabelText('Nom du bien'), 'Studio')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getAllByText('Ce champ est obligatoire.')).toHaveLength(2)
    expect(calls.count).toBe(0)
  })

  it('présélectionne le bien quand on ajoute un prêt depuis sa fiche', async () => {
    mockRealEstate()
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ ...loanAccount, id: 'a-travaux', holdings: [], property: null, loan: buildRealEstateLoan() })
      }),
    )
    const { user: actor } = await openRealEstate()
    await actor.click(within(propertyCard('Maison')).getByRole('button', { name: 'Ajouter un prêt' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un prêt' }))
    expect(await dialog.findByLabelText('Bien financé')).toHaveValue('a-maison')

    await actor.type(dialog.getByLabelText('Nom du prêt'), 'Prêt travaux')
    await actor.type(dialog.getByLabelText('Banque prêteuse'), 'Banque du Canal')
    await actor.type(dialog.getByLabelText('Capital emprunté'), '20 000')
    await actor.type(dialog.getByLabelText('Durée'), '120')
    expect(dialog.getByText('Soit 10 ans.')).toBeInTheDocument()
    await actor.type(dialog.getByLabelText('Taux nominal annuel'), '3,5')
    await actor.type(dialog.getByLabelText('Taux d’assurance annuel'), '0,3')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({
      name: 'Prêt travaux',
      kind: 'loan',
      owners: [{ member_id: camille.id, share: 1 }],
      institution: 'Banque du Canal',
      currency: 'EUR',
      loan: {
        property_account_id: 'a-maison',
        principal: 20_000,
        interest_rate_pct: 3.5,
        insurance_rate_pct: 0.3,
        duration_months: 120,
        start_date: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      },
    })
  })

  it('affiche le refus de l’API dans le formulaire du prêt', async () => {
    mockRealEstate()
    mockApiError('post', '/api/accounts', 422, 'La durée du prêt doit être positive.')
    const { user: actor } = await openRealEstate()
    await actor.click(within(propertyCard('Maison')).getByRole('button', { name: 'Ajouter un prêt' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un prêt' }))
    await actor.type(await dialog.findByLabelText('Nom du prêt'), 'Prêt travaux')
    await actor.type(dialog.getByLabelText('Capital emprunté'), '20 000')
    await actor.type(dialog.getByLabelText('Durée'), '120')
    await actor.type(dialog.getByLabelText('Taux nominal annuel'), '3,5')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(await dialog.findByText('La durée du prêt doit être positive.')).toBeInTheDocument()
  })
})

describe('tableau d’amortissement', () => {
  it('regroupe l’échéancier par année puis le détaille par mois', async () => {
    mockRealEstate()
    mockJson('get', '/api/loans/a-pret/schedule', { loan: buildRealEstateLoan(), rows: buildScheduleRows(1200, 12) })
    const { user: actor } = await openRealEstate()
    await actor.click(within(propertyCard('Maison')).getByRole('button', { name: 'Échéancier' }))

    const dialog = await screen.findByRole('dialog', { name: 'Amortissement : Prêt maison' })
    const scope = within(dialog)
    const table = await scope.findByRole('table')
    expect(pageText(dialog)).toContain('Restant dû600 €')

    // Février 2026 à janvier 2027 : 11 échéances en 2026, 1 en 2027.
    const years = within(table).getAllByRole('row').slice(1)
    expect(years.map((row) => pageText(row))).toEqual([
      '20261 100 €1 100 €0 €0 €100 €',
      '2027100 €100 €0 €0 €0 €',
    ])

    await actor.click(scope.getByRole('button', { name: 'Par mois' }))
    const months = within(table).getAllByRole('row').slice(1)
    expect(months).toHaveLength(12)
    expect(pageText(months[0])).toContain('1. févr. 2026')
    expect(pageText(months[11])).toContain('12. janv. 2027')
  })

  it('affiche l’erreur de l’API dans la fenêtre de l’échéancier', async () => {
    mockRealEstate()
    mockApiError('get', '/api/loans/a-pret/schedule', 404, 'Prêt introuvable.')
    const { user: actor } = await openRealEstate()
    await actor.click(within(propertyCard('Maison')).getByRole('button', { name: 'Échéancier' }))
    const dialog = await screen.findByRole('dialog', { name: 'Amortissement : Prêt maison' })
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Prêt introuvable.')
  })
})

describe('simulateur de prêt', () => {
  it('calcule mensualité, intérêts et coût total au fil de la saisie', async () => {
    mockRealEstate()
    const { user: actor } = await openRealEstate()
    const simulator = within(panel('Simulateur de prêt'))
    expect(pageText(panel('Simulateur de prêt'))).toContain('Renseignez le montant, le taux et la durée')

    await actor.type(simulator.getByLabelText('Montant emprunté'), '100 000')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '3')
    await actor.type(simulator.getByLabelText('Durée'), '20')
    let text = pageText(panel('Simulateur de prêt'))
    expect(text).toContain('Mensualité554,60 €sur 20 ans')
    expect(text).toContain('Intérêts33 103 €')
    expect(text).toContain('Coût total du crédit33 103 €133 103 € remboursés au total')

    await actor.type(simulator.getByLabelText('Taux d’assurance annuel'), '0,3')
    text = pageText(panel('Simulateur de prêt'))
    expect(text).toContain('Mensualité579,60 €dont 25,00 € d’assurance')
    expect(text).toContain('Assurance6 000 €')
    expect(text).toContain('Coût total du crédit39 103 €139 103 € remboursés au total')
  })

  it('n’affiche aucun résultat pour une durée hors limites', async () => {
    mockRealEstate()
    const { user: actor } = await openRealEstate()
    const simulator = within(panel('Simulateur de prêt'))
    await actor.type(simulator.getByLabelText('Montant emprunté'), '100 000')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '3')
    await actor.type(simulator.getByLabelText('Durée'), '60')
    expect(pageText(panel('Simulateur de prêt'))).toContain('(50 ans au plus)')
    expect(pageText(panel('Simulateur de prêt'))).not.toContain('Coût total du crédit')
  })
})
