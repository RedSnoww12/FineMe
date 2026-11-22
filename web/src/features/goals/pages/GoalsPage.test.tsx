import { describe, expect, it } from 'vitest'
import { buildAccount, buildGoal, buildOverview } from '@/test/fixtures'
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

  describe('profil investisseur', () => {
    function mockProfileScreen() {
      mockShell()
      mockJson('get', '/api/accounts', [buildAccount()])
      mockJson('get', '/api/goals', [safetyNet])
      mockJson('get', '/api/overview', buildOverview())
    }

    it('affiche la fourchette recommandée à côté de l’objectif de matelas', async () => {
      mockProfileScreen()
      renderApp('/objectifs')
      expect(await screen.findByText('Matelas recommandé : 3 à 6 mois de dépenses')).toBeInTheDocument()
      expect((await screen.findByText('Recommandé pour votre profil')).nextElementSibling).toHaveTextContent('3 à 6 mois')
      expect(await screen.findByText(/dans la fourchette recommandée/)).toBeInTheDocument()
      expect(pageText()).toContain('Votre couverture visée est dans la fourchette recommandée.')
      expect(pageText()).toContain('ce n’est pas un conseil en investissement')
    })

    it('recalcule la fourchette selon les réponses et la garde sur l’appareil', async () => {
      mockProfileScreen()
      const { user, unmount } = renderApp('/objectifs')
      await user.selectOptions(await screen.findByLabelText('Stabilité des revenus'), 'variable')
      await user.click(screen.getByLabelText('Propriétaire de votre logement'))
      // Revenus variables et propriétaire : 3 + 3 + 0 = 6 à 6 + 6 + 1 = 13 mois.
      expect(screen.getByText('Matelas recommandé : 6 à 13 mois de dépenses')).toBeInTheDocument()
      expect(screen.getByText('Propriétaire : travaux et gros entretien')).toBeInTheDocument()
      // Trois personnes à charge : 8 à 16 mois, les 6 mois visés sont en dessous.
      await user.selectOptions(screen.getByLabelText('Personnes à charge'), 'many')
      expect(await screen.findByText(/en dessous de la fourchette/)).toBeInTheDocument()
      expect(pageText()).toContain('Votre couverture visée est en dessous de la fourchette recommandée.')

      unmount()
      mockProfileScreen()
      renderApp('/objectifs')
      expect(await screen.findByText('Matelas recommandé : 8 à 16 mois de dépenses')).toBeInTheDocument()
    })

    it('compare l’allocation actuelle au profil déclaré', async () => {
      mockProfileScreen()
      const { user } = renderApp('/objectifs')
      // 33 000 de titres pour 12 000 d'épargne : 73 % ; profil moyen à moyen terme : 50 %.
      expect(await screen.findByText(/Titres et crypto/)).toHaveTextContent('73 % aujourd’hui, environ 50 % pour ce profil')
      await user.selectOptions(screen.getByLabelText('Tolérance au risque'), 'low')
      expect(screen.getByText(/Titres et crypto/)).toHaveTextContent('environ 25 % pour ce profil')
    })
  })
})
