import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { BudgetFlows, BudgetMonth, BudgetTransaction } from '@/shared/api/types'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

function flows(month: string, income = 0, expenses = 0): BudgetFlows {
  return { month, income, expenses, savings: income - expenses, savings_rate: income > 0 ? (income - expenses) / income : null }
}

function buildMonth(overrides: Partial<BudgetMonth> = {}): BudgetMonth {
  return {
    ...flows('2026-03', 3200, 2000),
    categories: [
      { category: 'Logement', amount: 1100, target: null },
      { category: 'Alimentation', amount: 450, target: 400 },
      { category: null, amount: 450, target: null },
    ],
    trend: Array.from({ length: 12 }, (_, i) => flows(`2025-${String(i + 1).padStart(2, '0')}`, i === 11 ? 3200 : 0, i === 11 ? 2000 : 0)),
    recurring: [
      { label: 'PRLV ENERGIE DEMO', category: 'Logement', average_amount: 62.5, occurrences: 4, last_date: '2026-03-05', next_date: '2026-04-05' },
    ],
    internal_transfers: 2,
    ...overrides,
  }
}

function buildTransaction(overrides: Partial<BudgetTransaction> = {}): BudgetTransaction {
  return {
    id: 't-1',
    account_id: 'a-1',
    account_name: 'Compte joint',
    date: '2026-03-12',
    label: 'CB LIBRAIRIE DEMO',
    amount: -24,
    amount_eur: -24,
    category: null,
    category_source: null,
    internal_transfer: false,
    internal_transfer_manual: false,
    ...overrides,
  }
}

function mockBudget(month: BudgetMonth, transactions: BudgetTransaction[]) {
  mockShell()
  mockJson('get', '/api/budget', month)
  mockJson('get', '/api/budget/transactions', transactions)
  mockJson('get', '/api/budget/categories', ['Alimentation', 'Culture', 'Logement'])
  mockJson('get', '/api/budget/rules', [])
}

describe('budget', () => {
  it('affiche les flux du mois, les catégories avec cibles et les charges récurrentes', async () => {
    mockBudget(buildMonth(), [buildTransaction()])
    renderApp('/budget?mois=2026-03')

    expect(await screen.findByRole('heading', { name: 'Budget' })).toBeInTheDocument()
    await screen.findByText('Dépenses par catégorie')
    const text = pageText()
    expect(text).toContain('Revenus3 200 €')
    expect(text).toContain('Épargne+1 200 €')
    expect(text).toContain('Taux d’épargne38 %')
    expect(text).toContain('2 virements internes exclus des calculs.')
    expect(text).toContain('Cible dépassée de 50 €')
    expect(text).toContain('Non catégorisé')
    expect(text).toContain('PRLV ENERGIE DEMO')
    expect(screen.getByLabelText('Mois affiché')).toHaveValue('2026-03')
    expect(screen.getByRole('link', { name: 'Budget' })).toHaveAttribute('aria-current', 'page')
  })

  it('recatégorise une transaction et envoie seulement ce qui change', async () => {
    mockBudget(buildMonth(), [buildTransaction()])
    const bodies: unknown[] = []
    server.use(
      http.put(`${API}/api/transactions/:id`, async ({ request, params }) => {
        bodies.push({ id: params.id, body: await request.json() })
        return HttpResponse.json({})
      }),
    )
    const { user } = renderApp('/budget?mois=2026-03')
    await user.click(await screen.findByRole('button', { name: 'Catégoriser « CB LIBRAIRIE DEMO »' }))
    const dialog = screen.getByRole('dialog')
    await user.type(within(dialog).getByLabelText('Catégorie'), ' Culture ')
    await user.selectOptions(within(dialog).getByLabelText('Virement interne'), 'no')
    await user.click(within(dialog).getByRole('button', { name: 'Enregistrer' }))

    await waitFor(() => expect(bodies).toEqual([{ id: 't-1', body: { category: 'Culture', internal_transfer: false } }]))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('crée une cible mensuelle et valide la saisie', async () => {
    mockBudget(buildMonth(), [buildTransaction()])
    const bodies: unknown[] = []
    server.use(
      http.put(`${API}/api/budget/targets`, async ({ request }) => {
        bodies.push(await request.json())
        return HttpResponse.json([])
      }),
    )
    const { user } = renderApp('/budget?mois=2026-03')
    await user.click(await screen.findByRole('button', { name: 'Ajouter une cible' }))
    const dialog = screen.getByRole('dialog')
    await user.click(within(dialog).getByRole('button', { name: 'Enregistrer' }))
    expect(within(dialog).getByText('Indiquez une catégorie.')).toBeInTheDocument()
    expect(bodies).toEqual([])
    await user.type(within(dialog).getByLabelText('Catégorie'), 'Culture')
    await user.type(within(dialog).getByLabelText('Montant mensuel visé'), '80,5')
    await user.click(within(dialog).getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(bodies).toEqual([{ category: 'Culture', monthly_amount: 80.5 }]))
  })

  it('change de mois avec les boutons', async () => {
    mockShell()
    const months: (string | null)[] = []
    server.use(
      http.get(`${API}/api/budget`, ({ request }) => {
        const month = new URL(request.url).searchParams.get('month')
        months.push(month)
        return HttpResponse.json(buildMonth({ ...flows(month ?? '') }))
      }),
    )
    mockJson('get', '/api/budget/transactions', [buildTransaction()])
    mockJson('get', '/api/budget/categories', [])
    mockJson('get', '/api/budget/rules', [])
    const { user } = renderApp('/budget?mois=2026-01')
    await user.click(await screen.findByRole('button', { name: 'Mois précédent' }))
    await waitFor(() => expect(months).toContain('2025-12'))
    expect(screen.getByLabelText('Mois affiché')).toHaveValue('2025-12')
  })

  it('invite à importer des transactions quand le mois est vide', async () => {
    const empty = buildMonth({ ...flows('2026-03'), categories: [], recurring: [], internal_transfers: 0, trend: [] })
    mockBudget(empty, [])
    renderApp('/budget?mois=2026-03')
    expect(await screen.findByRole('heading', { name: 'Aucune transaction en mars 2026' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Importer des transactions depuis un compte' })).toHaveAttribute('href', '/comptes')
  })

  it('affiche l’erreur de l’API', async () => {
    mockBudget(buildMonth(), [])
    mockApiError('get', '/api/budget', 500, 'Base indisponible.')
    renderApp('/budget?mois=2026-03')
    expect(await screen.findByRole('alert')).toHaveTextContent('Base indisponible.')
    expect(screen.getByRole('button', { name: 'Réessayer' })).toBeInTheDocument()
  })
})
