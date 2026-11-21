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

describe('simulation sur l’échéancier', () => {
  async function openSchedule() {
    mockRealEstate()
    mockJson('get', '/api/loans/a-pret/schedule', { loan: buildRealEstateLoan(), rows: buildScheduleRows(1200, 12) })
    const view = await openRealEstate()
    await view.user.click(within(propertyCard('Maison')).getByRole('button', { name: 'Échéancier' }))
    const dialog = await screen.findByRole('dialog', { name: 'Amortissement : Prêt maison' })
    await within(dialog).findByRole('table')
    return { ...view, dialog, scope: within(dialog) }
  }

  it('raccourcit l’échéancier après un remboursement anticipé partiel', async () => {
    const { user: actor, dialog, scope } = await openSchedule()
    await actor.click(scope.getByRole('button', { name: 'Remboursement anticipé' }))
    await actor.type(scope.getByLabelText('Après l’échéance n°'), '2')
    await actor.type(scope.getByLabelText('Montant remboursé'), '300')

    // Après 2 échéances il reste 1 000 € ; 300 € remboursés laissent 7 mensualités de 100 €.
    const text = pageText(dialog)
    expect(text).toContain('Nouvelle finoct. 20263 mois plus tôt')
    expect(text).toContain('Indemnités, plafond légal0 €300 € remboursés par anticipation')
    expect(scope.getByRole('heading', { name: 'Échéancier simulé' })).toBeInTheDocument()
    const years = within(scope.getByRole('table')).getAllByRole('row').slice(1)
    expect(years.map((row) => pageText(row))).toEqual(['2026900 €900 €0 €0 €300 €0 €'])
  })

  it('baisse la mensualité en gardant la durée', async () => {
    const { user: actor, dialog, scope } = await openSchedule()
    await actor.click(scope.getByRole('button', { name: 'Remboursement anticipé' }))
    await actor.type(scope.getByLabelText('Après l’échéance n°'), '2')
    await actor.type(scope.getByLabelText('Montant remboursé'), '300')
    await actor.selectOptions(scope.getByLabelText('Ensuite'), 'reduce_payment')
    expect(pageText(dialog)).toContain('Nouvelle mensualité70,00 €-30,00 € par mois, hors assurance')
    expect(pageText(dialog)).toContain('Nouvelle finjanv. 2027durée inchangée')
  })

  it('solde le prêt par un remboursement total', async () => {
    const { user: actor, dialog, scope } = await openSchedule()
    await actor.click(scope.getByRole('button', { name: 'Remboursement anticipé' }))
    await actor.selectOptions(scope.getByLabelText('Remboursement'), 'total')
    await actor.type(scope.getByLabelText('Après l’échéance n°'), '6')
    expect(pageText(dialog)).toContain('Nouvelle mensualitéPrêt soldé')
    await actor.click(scope.getByRole('button', { name: 'Par mois' }))
    const months = within(scope.getByRole('table')).getAllByRole('row').slice(1)
    expect(months).toHaveLength(6)
    expect(pageText(months[5])).toContain('600,00 €0,00 €')
  })

  it('n’applique rien tant que l’échéance est hors du prêt', async () => {
    const { user: actor, scope } = await openSchedule()
    await actor.click(scope.getByRole('button', { name: 'Renégociation' }))
    await actor.type(scope.getByLabelText('Après l’échéance n°'), '40')
    await actor.type(scope.getByLabelText('Nouveau taux nominal'), '1')
    expect(scope.getByRole('heading', { name: 'Échéancier' })).toBeInTheDocument()
    expect(scope.queryByText('Gain net des frais')).not.toBeInTheDocument()
  })
})

async function openSimulator(name: string) {
  mockRealEstate()
  const view = await openRealEstate()
  await view.user.click(screen.getByRole('button', { name: 'Simulateurs' }))
  await view.user.click(screen.getByRole('button', { name }))
  return view
}

describe('simulateur de prêt', () => {
  it('calcule mensualité, intérêts et coût total au fil de la saisie', async () => {
    const { user: actor } = await openSimulator('Prêt')
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
    const { user: actor } = await openSimulator('Prêt')
    const simulator = within(panel('Simulateur de prêt'))
    await actor.type(simulator.getByLabelText('Montant emprunté'), '100 000')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '3')
    await actor.type(simulator.getByLabelText('Durée'), '60')
    expect(pageText(panel('Simulateur de prêt'))).toContain('(50 ans au plus)')
    expect(pageText(panel('Simulateur de prêt'))).not.toContain('Coût total du crédit')
  })
})

describe('onglet des simulateurs', () => {
  it('s’ouvre depuis l’adresse et masque les actions des biens', async () => {
    mockRealEstate()
    renderApp('/immobilier?vue=simulateurs')
    expect(await screen.findByRole('heading', { name: 'Investissement locatif' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Simulateurs' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.queryByRole('button', { name: 'Ajouter un bien' })).not.toBeInTheDocument()
  })
})

describe('simulateur d’investissement locatif', () => {
  it('affiche rendements, cash-flow, revente et TRI', async () => {
    const { user: actor } = await openSimulator('Investissement locatif')
    const simulator = within(panel('Investissement locatif'))
    expect(pageText(panel('Investissement locatif'))).toContain('Renseignez le prix, l’apport, le loyer')

    await actor.type(simulator.getByLabelText('Prix d’achat'), '100 000')
    await actor.type(simulator.getByLabelText('Frais d’acquisition'), '8 000')
    await actor.type(simulator.getByLabelText('Travaux'), '2 000')
    await actor.type(simulator.getByLabelText('Apport personnel'), '10 000')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '0')
    await actor.type(simulator.getByLabelText('Durée du prêt'), '20')
    await actor.type(simulator.getByLabelText('Loyer mensuel hors charges'), '800')
    await actor.type(simulator.getByLabelText('Charges mensuelles non récupérables'), '50')
    await actor.type(simulator.getByLabelText('Taxe foncière annuelle'), '600')
    await actor.type(simulator.getByLabelText('Horizon de détention'), '10')

    // 100 000 € sur 240 mois à taux nul : 416,67 € ; loyer net 700 € par mois.
    const text = pageText(panel('Investissement locatif'))
    expect(text).toContain('Rendement brut8,73 %sur 110 000 € de prix de revient')
    expect(text).toContain('Rendement net de charges7,64 %')
    expect(text).toContain('Cash-flow mensuel+283 €après 416,67 € de mensualité')
    expect(text).toContain('Effort d’épargne mensuel0 €le loyer couvre tout')
    // Revente 102 000 € moins 50 000 € restant dû ; gain -10 000 + 10 × 3 400 + 52 000.
    expect(text).toContain('Revente nette52 000 €')
    expect(text).toContain('Gain net sur l’horizon+76 000 €')
    expect(text).toContain('TRI sur 10 ans')
  })

  it('mesure l’effort d’épargne d’un cash-flow négatif', async () => {
    const { user: actor } = await openSimulator('Investissement locatif')
    const simulator = within(panel('Investissement locatif'))
    await actor.type(simulator.getByLabelText('Prix d’achat'), '100 000')
    await actor.type(simulator.getByLabelText('Apport personnel'), '0')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '0')
    await actor.type(simulator.getByLabelText('Durée du prêt'), '10')
    await actor.type(simulator.getByLabelText('Loyer mensuel hors charges'), '500')
    await actor.type(simulator.getByLabelText('Horizon de détention'), '10')
    // Mensualité 833,33 € pour 500 € de loyer.
    expect(pageText(panel('Investissement locatif'))).toContain('Effort d’épargne mensuel333 €à compléter chaque mois')
  })

  it('reprend les données d’un bien enregistré', async () => {
    const { user: actor } = await openSimulator('Investissement locatif')
    const simulator = within(panel('Investissement locatif'))
    await actor.selectOptions(simulator.getByLabelText('Partir d’un bien enregistré'), 'a-maison')
    // Prix 1 800 € + 100 € de frais, prêt de 1 200 € sur 12 mois : apport 700 €.
    expect(simulator.getByLabelText('Prix d’achat')).toHaveValue('1800')
    expect(simulator.getByLabelText('Frais d’acquisition')).toHaveValue('100')
    expect(simulator.getByLabelText('Apport personnel')).toHaveValue('700')
    expect(simulator.getByLabelText('Taux nominal annuel')).toHaveValue('0')
    expect(simulator.getByLabelText('Durée du prêt')).toHaveValue('1')
  })
})

describe('simulateur de capacité d’emprunt', () => {
  it('calcule la capacité sous le taux d’endettement retenu', async () => {
    const { user: actor } = await openSimulator('Capacité d’emprunt')
    const simulator = within(panel('Capacité d’emprunt'))
    await actor.type(simulator.getByLabelText('Revenus nets mensuels du foyer'), '4 000')
    await actor.type(simulator.getByLabelText('Mensualités des crédits en cours'), '200')
    await actor.type(simulator.getByLabelText('Taux d’endettement maximal'), '35')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '0')
    await actor.type(simulator.getByLabelText('Durée'), '20')
    const text = pageText(panel('Capacité d’emprunt'))
    expect(text).toContain('Taux d’endettement actuel5,0 %')
    expect(text).toContain('Mensualité disponible1 200,00 €')
    expect(text).toContain('Capacité d’emprunt288 000 €')
    expect(text).toContain('Taux d’endettement après emprunt35,0 %')
  })
})

describe('simulateur acheter ou louer', () => {
  it('compare les patrimoines à l’horizon', async () => {
    const { user: actor } = await openSimulator('Acheter ou louer')
    const simulator = within(panel('Acheter ou louer'))
    await actor.type(simulator.getByLabelText('Prix d’achat'), '100 000')
    await actor.type(simulator.getByLabelText('Taux nominal annuel'), '0')
    await actor.type(simulator.getByLabelText('Durée du prêt'), '10')
    await actor.type(simulator.getByLabelText('Loyer mensuel d’un logement équivalent'), '500')
    await actor.type(simulator.getByLabelText('Horizon de comparaison'), '10')
    const text = pageText(panel('Acheter ou louer'))
    expect(text).toContain('Patrimoine en achetant100 000 €')
    expect(text).toContain('Patrimoine en louant40 000 €')
    expect(text).toContain('Avantage à l’achat60 000 €')
    expect(text).toContain('Achat rentabiliséAprès 1 an')
  })
})
