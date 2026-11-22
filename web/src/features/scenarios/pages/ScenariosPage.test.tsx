import { waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { money } from '@/shared/lib/format'
import { buildAccount, buildGoal, buildOverview, emptyOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockApiError, mockJson } from '@/test/server'

const plain = (text: string) => text.replace(/[\u00a0\u202f]/g, ' ')

// Patrimoine net 120 000 : 33 000 d'investissements, 12 000 d'épargne, 135 000 d'immobilier, 60 000 de dettes.
function mockScreen() {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/accounts', [buildAccount({ id: 'pea', kind: 'investment', value: 33_000 })])
  mockJson('get', '/api/goals', [
    buildGoal({
      name: 'Retraite',
      account_ids: ['pea'],
      progress: { ...buildGoal().progress, current_amount: 33_000, target_amount: 66_000, pct: 50 },
    }),
    buildGoal({ id: 'g-2', name: 'Matelas', kind: 'safety_net', monthly_expenses: 3_000, account_ids: [] }),
  ])
}

describe('simulateur « Et si ? »', () => {
  it('affiche instantanément l’effet d’un krach de 30 % sur le patrimoine net et les objectifs', async () => {
    mockScreen()
    const { user } = renderApp('/et-si')
    await user.click(await screen.findByRole('button', { name: 'Krach des actions' }))

    expect(screen.getByLabelText('Variation des investissements')).toHaveValue('-30')
    // 33 000 × 0,7 = 23 100 : le patrimoine net perd 9 900.
    await waitFor(() => expect(pageText()).toContain(plain(money(110_100))))
    expect(pageText()).toContain(plain(`-${money(9_900)}`))
    // L'objectif adossé au PEA passe de 50 % à 35 %.
    expect(pageText()).toContain('50 % → 35 %')
  })

  it('reprend les dépenses du matelas et chiffre les mois couverts', async () => {
    mockScreen()
    const { user } = renderApp('/et-si')
    expect(await screen.findByLabelText('Dépenses mensuelles')).toHaveValue('3000')
    // 12 000 d'épargne pour 3 000 € par mois : 4 mois, 2 après deux mois sans revenus.
    expect(screen.getByText('avant : 4')).toBeInTheDocument()
    await user.type(screen.getByLabelText('Mois sans revenus'), '2')
    expect(await screen.findByText('2', { selector: '.num' })).toBeInTheDocument()
  })

  it('se réinitialise sans rien enregistrer', async () => {
    mockScreen()
    const { user } = renderApp('/et-si')
    await user.click(await screen.findByRole('button', { name: 'Héritage' }))
    expect(screen.getByLabelText('Héritage ou don reçu')).toHaveValue('50000')
    await user.click(screen.getByRole('button', { name: 'Réinitialiser' }))
    expect(screen.getByLabelText('Héritage ou don reçu')).toHaveValue('')
    expect(pageText()).toContain(plain(money(120_000)))
  })

  it('invite à ajouter des comptes quand il n’y a rien à simuler', async () => {
    mockShell()
    mockJson('get', '/api/overview', emptyOverview)
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    renderApp('/et-si')
    expect(await screen.findByText('Aucun patrimoine à simuler')).toBeInTheDocument()
  })

  it('affiche l’erreur de l’API', async () => {
    mockShell()
    mockApiError('get', '/api/overview', 500, 'Base indisponible.')
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    renderApp('/et-si')
    expect(await screen.findByText(/Base indisponible/)).toBeInTheDocument()
  })
})
