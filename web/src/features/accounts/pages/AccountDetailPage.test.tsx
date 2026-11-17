import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildAccountDetail, buildHolding } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { mockJson } from '@/test/server'

function mockInvestmentAccount(holdings: ReturnType<typeof buildHolding>[]) {
  mockShell()
  mockJson(
    'get',
    '/api/accounts/a-pea',
    buildAccountDetail({ id: 'a-pea', name: 'PEA Camille', kind: 'investment', envelope: 'pea', holdings }),
  )
  mockJson('get', '/api/accounts/a-pea/transactions', { items: [], total: 0 })
}

describe('positions d’un compte', () => {
  it('signale les cours automatiques qui ne sont plus mis à jour', async () => {
    mockInvestmentAccount([
      buildHolding(),
      buildHolding({ id: 'h-2', name: 'Action Démo', symbol: 'DEMO', price_stale: true }),
    ])
    renderApp('/comptes/a-pea')

    const stale = await screen.findByRole('row', { name: /Action Démo/ })
    expect(within(stale).getByText('ancien')).toBeInTheDocument()
    const fresh = screen.getByRole('row', { name: /Fonds Indice Monde/ })
    expect(within(fresh).queryByText('ancien')).not.toBeInTheDocument()
    expect(
      screen.getByText(/Un cours n’est plus mis à jour automatiquement depuis plusieurs jours/),
    ).toBeInTheDocument()
  })

  it('n’affiche aucun avertissement quand les cours sont à jour ou saisis', async () => {
    mockInvestmentAccount([
      buildHolding(),
      buildHolding({ id: 'h-2', name: 'Fonds euro', kind: 'fonds_euro', price_source: 'manual' }),
    ])
    renderApp('/comptes/a-pea')

    const manual = await screen.findByRole('row', { name: /Fonds euro/ })
    expect(within(manual).getByText('saisi')).toBeInTheDocument()
    expect(screen.queryByText('ancien')).not.toBeInTheDocument()
    expect(screen.queryByText(/plus mis à jour automatiquement/)).not.toBeInTheDocument()
  })
})
