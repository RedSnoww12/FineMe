import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { HistoryPoint } from '@/shared/api/types'
import {
  buildAccount,
  buildGoal,
  buildHistoryAccount as line,
  buildHistoryPoint as point,
  buildOverview,
  buildRealEstate,
  buildScheduleRows,
  camille,
  emptyOverview,
} from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockApiError, mockJson } from '@/test/server'

// Début : 10 000 € de PEA (1 000 € de plus-value), 5 000 € de prêt. Fin : +1 500 € de plus-value, 800 € remboursés.
const history: HistoryPoint[] = [
  point('2026-03-01', [
    line({ account_id: 'pea', name: 'PEA', kind: 'investment', value: 10_000, gain: 1000 }),
    line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 5000 }),
  ]),
  point('2026-03-02', [
    line({ account_id: 'pea', name: 'PEA', kind: 'investment', value: 11_500, gain: 2500 }),
    line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 4200 }),
  ]),
]

const pea = buildAccount({
  id: 'pea',
  name: 'PEA Camille',
  kind: 'investment',
  envelope: 'pea',
  value: 11_500,
  owned_value: 11_500,
  invested: 9000,
  gain: 2500,
  owners: [{ member_id: camille.id, name: camille.name, share: 1 }],
})

// Prêt de la maison des fixtures, détenu à 50/50 par Camille et Sacha.
const loan = buildAccount({ id: 'a-pret', name: 'Prêt maison', kind: 'loan', is_liability: true, value: 600, owned_value: 600 })

function mockAnalysis({ points = history }: { points?: HistoryPoint[] } = {}) {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/accounts', [pea, loan])
  mockJson('get', '/api/goals', [buildGoal({ account_ids: ['pea'] })])
  mockJson('get', '/api/history', { points })
  mockJson('get', '/api/real-estate', buildRealEstate())
  mockJson('get', '/api/loans/a-pret/schedule', { loan: buildRealEstate().properties[0].loans[0], rows: buildScheduleRows(1200, 12) })
}

async function panel(title: string) {
  const heading = await screen.findByRole('heading', { name: title })
  const section = heading.closest('section')
  if (!section) throw new Error(`Panneau « ${title} » introuvable`)
  return section
}

describe('analyse', () => {
  it('présente chaque graphique avec un titre, une explication et une alternative tabulaire', async () => {
    mockAnalysis()
    renderApp('/analyse')

    for (const title of [
      'Évolution des actifs par catégorie',
      'Actifs, dettes et patrimoine net',
      'Variation sur la période',
      'Répartition des actifs',
      'Performance',
      'Immobilier et prêts',
      'Objectifs',
    ]) {
      expect(await panel(title)).toBeInTheDocument()
    }
    expect(await screen.findAllByText('Voir les données en tableau')).toHaveLength(6)
    expect(screen.getAllByRole('img').length).toBeGreaterThanOrEqual(6)
  })

  it('décompose la variation du patrimoine net, dans un tableau lisible au clavier', async () => {
    mockAnalysis()
    const { user } = renderApp('/analyse')
    const variation = await panel('Variation sur la période')
    await user.click(within(variation).getByText('Voir les données en tableau'))
    const table = within(variation).getByRole('table', { name: 'Décomposition de la variation du patrimoine net' })
    const text = pageText(table)
    expect(text).toContain('Début de période5 000 €')
    expect(text).toContain('Performance des placements+1 500 €')
    expect(text).toContain('Remboursement de dettes+800 €')
    expect(text).toContain('Fin de période7 300 €')
  })

  it('bascule tous les graphiques en pourcentage', async () => {
    mockAnalysis()
    const { user } = renderApp('/analyse')
    const variation = await panel('Variation sur la période')
    await user.click(screen.getByRole('button', { name: 'En %' }))
    expect(screen.getByRole('button', { name: 'En %' })).toHaveAttribute('aria-pressed', 'true')
    await waitFor(() => expect(pageText(variation)).toContain('Performance des placements+30,0 %'))
    expect(pageText(await panel('Évolution des actifs par catégorie'))).toContain('100,0 %')
  })

  it('répartit les actifs par enveloppe, sans proposer l’axe des membres dans la vue d’un membre', async () => {
    localStorage.setItem('fineme.view', camille.id)
    mockAnalysis()
    const { user } = renderApp('/analyse')
    const breakdown = await panel('Répartition des actifs')
    expect(within(breakdown).queryByRole('button', { name: 'Membre' })).toBeNull()
    await user.click(within(breakdown).getByRole('button', { name: 'Enveloppe' }))
    const table = within(breakdown).getByRole('table', { name: 'Répartition des actifs par enveloppe' })
    expect(pageText(table)).toContain('PEA18,3 %33 000 €')
    expect(pageText(table)).toContain('Sans enveloppe81,7 %147 000 €')
    expect(pageText()).toContain('Graphiques détaillés de la part détenue par Camille.')
  })

  it('mesure la performance par compte, à la part détenue', async () => {
    mockAnalysis()
    renderApp('/analyse')
    const performance = await panel('Performance')
    await waitFor(() => expect(pageText(performance)).toContain('PEA Camille'))
    expect(pageText(performance)).toContain('+2 500 €')
    expect(pageText(performance)).toContain('+27,8 %')
  })

  it('suit le capital remboursé et la valeur nette des biens', async () => {
    mockAnalysis()
    renderApp('/analyse')
    const realEstate = await panel('Immobilier et prêts')
    await waitFor(() => expect(within(realEstate).getByRole('table')).toBeInTheDocument())
    // Ligne de janvier 2026 : 2 000 € de valeur, 1 200 € restant dus avant la première échéance.
    expect(pageText(within(realEstate).getByRole('table'))).toContain('0 €1 200 €800 €2 000 €')
  })

  it('refuse une période personnalisée dont le début suit la fin', async () => {
    mockAnalysis()
    const { user } = renderApp('/analyse')
    await user.selectOptions(await screen.findByLabelText('Période'), 'custom')
    const from = screen.getByLabelText('Du')
    await user.clear(from)
    await user.type(from, '2030-01-01')
    expect(await screen.findByText('La date de début doit précéder la date de fin.')).toBeInTheDocument()
  })

  it('explique que les graphiques d’historique se construisent avec le temps', async () => {
    mockAnalysis({ points: [history[0]] })
    renderApp('/analyse')
    expect(pageText(await panel('Variation sur la période'))).toContain('dès le deuxième jour de la période')
  })

  it('signale l’absence de détail par catégorie pour les anciens instantanés', async () => {
    const old = [point('2026-03-01', null, { gross_assets: 1000, net_worth: 1000 }), point('2026-03-02', null, { gross_assets: 1100, net_worth: 1100 })]
    mockAnalysis({ points: old })
    renderApp('/analyse')
    expect(pageText(await panel('Évolution des actifs par catégorie'))).toContain('au fil des prochains jours')
    expect(pageText(await panel('Variation sur la période'))).toContain('La performance n’est pas isolée')
  })

  it('affiche une erreur dans les graphiques d’historique sans masquer les autres', async () => {
    mockAnalysis()
    mockApiError('get', '/api/history', 500, 'Historique indisponible.')
    renderApp('/analyse')
    const evolution = await panel('Évolution des actifs par catégorie')
    expect(await within(evolution).findByRole('alert')).toHaveTextContent('Historique indisponible.')
    expect(pageText(await panel('Répartition des actifs'))).toContain('Immobilier')
  })

  it('invite à ajouter un compte quand il n’y a rien à analyser', async () => {
    mockShell()
    mockJson('get', '/api/overview', emptyOverview)
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    mockJson('get', '/api/history', { points: [] })
    renderApp('/analyse')
    expect(await screen.findByRole('heading', { name: 'Rien à analyser pour l’instant' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Ajouter un compte' })).toHaveAttribute('href', '/comptes')
  })
})
