import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildAccountDetail, buildHolding } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

function csvFile(name: string, lines: string[]) {
  return new File([lines.join('\n')], name, { type: 'text/csv' })
}

/** Ouvre l'import CSV depuis le panneau des opérations d'un compte d'investissement. */
async function openImport() {
  mockShell()
  mockJson('get', '/api/accounts/a-pea', buildAccountDetail({ id: 'a-pea', name: 'PEA Camille', kind: 'investment', envelope: 'pea' }))
  mockJson('get', '/api/accounts/a-pea/transactions', { items: [], total: 0 })
  const view = renderApp('/comptes/a-pea')
  await screen.findByRole('heading', { name: 'PEA Camille' })
  await view.user.click(screen.getAllByRole('button', { name: 'Importer un CSV' })[0])
  const dialog = await screen.findByRole('dialog', { name: 'Importer un fichier CSV' })
  return { ...view, dialog, scope: within(dialog) }
}

async function upload(actor: ReturnType<typeof renderApp>['user'], dialog: HTMLElement, file: File) {
  await actor.upload(within(dialog).getByLabelText('Fichier exporté depuis votre banque ou votre courtier'), file)
}

const BANK_STATEMENT = csvFile('releve.csv', [
  'Date;Libellé;Montant;Catégorie',
  '03/02/2025;Boulangerie du Port;-4,20;Alimentation',
  '05/02/2025;Virement salaire;2 100,00;Revenus',
  'pas une date;Ligne illisible;1,00;',
])

describe('import CSV des opérations', () => {
  it('détecte le séparateur, devine les colonnes, prévisualise et envoie les opérations', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts/a-pea/transactions/bulk`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ inserted: 1, skipped: 1 })
      }),
    )
    const { user: actor, dialog, scope } = await openImport()
    await upload(actor, dialog, BANK_STATEMENT)

    expect(await scope.findByText('releve.csv')).toBeInTheDocument()
    expect(pageText(dialog)).toContain('releve.csv : 3 lignes, 4 colonnes, séparateur point-virgule.')
    expect(scope.getByLabelText('Date')).toHaveValue('0')
    expect(scope.getByLabelText('Libellé')).toHaveValue('1')
    expect(scope.getByLabelText('Montant signé')).toHaveValue('2')
    expect(scope.getByLabelText('Catégorie')).toHaveValue('3')
    expect(scope.getByLabelText('Débit')).toHaveValue('-1')

    expect(scope.getByRole('heading', { name: 'Aperçu (2 sur 2)' })).toBeInTheDocument()
    const row = scope.getByRole('row', { name: /Boulangerie du Port/ })
    expect(pageText(row)).toContain('03/02/2025')
    expect(pageText(row)).toContain('-4,20 €')
    expect(pageText(dialog)).toContain('Ligne 4 : Date illisible : « pas une date »')

    await actor.type(scope.getByLabelText('Nouveau solde du compte après import'), '3 000')
    await actor.click(scope.getByRole('button', { name: 'Importer 2 opérations' }))

    expect(await scope.findByText('1 opération importée, 1 déjà présente ignorée.')).toBeInTheDocument()
    expect(received).toEqual({
      transactions: [
        { date: '2025-02-03', label: 'Boulangerie du Port', amount: -4.2, category: 'Alimentation' },
        { date: '2025-02-05', label: 'Virement salaire', amount: 2100, category: 'Revenus' },
      ],
      new_cash_balance: 3000,
    })
    // Fichier traité : l'aperçu disparaît, plus rien à importer.
    expect(scope.queryByRole('button', { name: /^Importer \d/ })).not.toBeInTheDocument()
  })

  it('demande la correspondance des colonnes non reconnues avant d’autoriser l’import', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts/a-pea/transactions/bulk`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ inserted: 2, skipped: 0 })
      }),
    )
    const { user: actor, dialog, scope } = await openImport()
    await upload(
      actor,
      dialog,
      csvFile('export.csv', ['Jour,Description,Sortie,Entrée', '2025-03-01,Loyer,650.00,', '2025-03-02,Remboursement,,12.50']),
    )

    expect(await scope.findByText(/séparateur virgule/)).toBeInTheDocument()
    expect(scope.getByRole('alert')).toHaveTextContent(
      'Choisissez la colonne de date. Choisissez une colonne de montant, ou des colonnes débit et crédit.',
    )
    expect(scope.getByRole('button', { name: 'Importer 0 opération' })).toBeDisabled()

    await actor.selectOptions(scope.getByLabelText('Date'), '0')
    await actor.selectOptions(scope.getByLabelText('Débit'), '2')
    await actor.selectOptions(scope.getByLabelText('Crédit'), '3')

    expect(scope.queryByRole('alert')).not.toBeInTheDocument()
    expect(scope.getByRole('heading', { name: 'Aperçu (2 sur 2)' })).toBeInTheDocument()
    expect(pageText(scope.getByRole('row', { name: /Loyer/ }))).toContain('-650,00 €')
    expect(pageText(scope.getByRole('row', { name: /Remboursement/ }))).toContain('+12,50 €')

    await actor.click(scope.getByRole('button', { name: 'Importer 2 opérations' }))
    expect(await scope.findByText('2 opérations importées.')).toBeInTheDocument()
    // Solde laissé vide : le champ n'est pas envoyé.
    expect(received).toEqual({
      transactions: [
        { date: '2025-03-01', label: 'Loyer', amount: -650 },
        { date: '2025-03-02', label: 'Remboursement', amount: 12.5 },
      ],
    })
  })

  it('refuse un fichier sans ligne de données sans appeler l’API', async () => {
    const calls = spyOn('post', '/api/accounts/a-pea/transactions/bulk')
    const { user: actor, dialog, scope } = await openImport()
    await upload(actor, dialog, csvFile('vide.csv', ['Date;Libellé;Montant']))

    expect(await scope.findByText('Ce fichier ne contient aucune ligne de données sous la ligne d’en-tête.')).toBeInTheDocument()
    expect(scope.queryByRole('button', { name: /^Importer \d/ })).not.toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('affiche le refus de l’API et garde l’aperçu', async () => {
    mockApiError('post', '/api/accounts/a-pea/transactions/bulk', 422, 'Le fichier contient trop d’opérations.')
    const { user: actor, dialog, scope } = await openImport()
    await upload(actor, dialog, BANK_STATEMENT)
    await actor.click(await scope.findByRole('button', { name: 'Importer 2 opérations' }))

    expect(await scope.findByText('Le fichier contient trop d’opérations.')).toBeInTheDocument()
    expect(scope.getByRole('heading', { name: 'Aperçu (2 sur 2)' })).toBeInTheDocument()
  })
})

describe('import CSV des positions', () => {
  it('lit un export de courtier et remplace les positions existantes', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/accounts/a-pea/holdings/bulk`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json([buildHolding(), buildHolding({ id: 'h-2', name: 'Action Démo' })])
      }),
    )
    const { user: actor, dialog, scope } = await openImport()
    await actor.click(scope.getByRole('button', { name: 'Positions' }))
    await upload(
      actor,
      dialog,
      csvFile('portefeuille.csv', [
        'Nom\tISIN\tQuantité\tPRU\tCours',
        'Fonds Indice Monde\tFR0000000001\t10\t300,00\t330,00',
        'Action Démo\tcode inconnu\t2,5\t40\t',
      ]),
    )

    expect(await scope.findByText(/séparateur tabulation/)).toBeInTheDocument()
    expect(scope.getByLabelText('Nom')).toHaveValue('0')
    expect(scope.getByLabelText('Cours actuel')).toHaveValue('4')
    expect(scope.getByRole('heading', { name: 'Aperçu (2 sur 2)' })).toBeInTheDocument()

    await actor.click(scope.getByRole('checkbox', { name: /Remplacer les positions existantes/ }))
    await actor.click(scope.getByRole('button', { name: 'Importer 2 positions' }))

    expect(await scope.findByText('2 positions importées.')).toBeInTheDocument()
    expect(received).toEqual({
      replace: true,
      holdings: [
        {
          kind: 'security',
          name: 'Fonds Indice Monde',
          isin: 'FR0000000001',
          quantity: 10,
          buy_price: 300,
          current_price: 330,
          currency: 'EUR',
        },
        // Sans cours, le prix d'achat sert de cours ; un ISIN invalide est ignoré.
        { kind: 'security', name: 'Action Démo', isin: null, quantity: 2.5, buy_price: 40, current_price: 40, currency: 'EUR' },
      ],
    })
  })
})
