import { describe, expect, it } from 'vitest'
import { buildAccount, buildGoal } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockJson, mockUnreachable } from '@/test/server'

const safetyNet = buildGoal({
  id: 'g-2',
  name: 'Réserve de précaution',
  kind: 'safety_net',
  target_amount: null,
  target_date: null,
  monthly_contribution: null,
  expected_return_pct: null,
  monthly_expenses: 2_000,
  coverage_months: 6,
  member_ids: [],
  progress: {
    current_amount: 12_000,
    target_amount: 12_000,
    pct: 100,
    status: 'funded',
    required_return_pct: null,
    projected_amount_at_date: null,
    coverage_months: 6,
  },
})

describe('objectifs', () => {
  it('liste les deux types d’objectifs avec progression et statut', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [buildAccount()])
    mockJson('get', '/api/goals', [buildGoal(), safetyNet])
    renderApp('/objectifs')

    expect(await screen.findByRole('heading', { name: 'Apport appartement' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Réserve de précaution' })).toBeInTheDocument()
    expect(screen.getByText('En bonne voie')).toBeInTheDocument()
    expect(screen.getByText('Financé')).toBeInTheDocument()

    const bars = screen.getAllByRole('progressbar')
    expect(bars.map((bar) => bar.getAttribute('aria-valuenow'))).toEqual(['25', '100'])

    const text = pageText()
    expect(text).toContain('sur 40 000 €, soit 25 %')
    expect(text).toContain('41 500 €') // montant projeté à l'échéance
    expect(text).toContain('3,2 % par an') // rendement nécessaire
    expect(text).toContain('4,0 % par an') // rendement attendu
    expect(text).toContain('Votre hypothèse de rendement suffit')
    expect(text).toContain('6 mois')
    expect(text).toContain('Comptes : Compte joint. Membres : Camille.')
  })

  it('signale un rendement attendu insuffisant', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [buildAccount()])
    const goal = buildGoal({ expected_return_pct: 2 })
    mockJson('get', '/api/goals', [{ ...goal, progress: { ...goal.progress, status: 'challenging', required_return_pct: 6.5 } }])
    renderApp('/objectifs')
    expect(await screen.findByText('Ambitieux')).toBeInTheDocument()
    expect(pageText()).toContain('Il manque 4,5 % de rendement annuel')
  })

  it('invite à créer un premier objectif quand la liste est vide', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    renderApp('/objectifs')
    expect(await screen.findByRole('heading', { name: 'Aucun objectif' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Créer un objectif' })).toBeInTheDocument()
  })

  it('affiche l’état d’erreur quand l’API est injoignable', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [])
    mockUnreachable('get', '/api/goals')
    renderApp('/objectifs')
    expect(await screen.findByRole('alert')).toHaveTextContent('API injoignable')
  })
})
