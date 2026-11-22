import { fireEvent, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildHistoryAccount as line, buildHistoryPoint as point, buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockApiError, mockJson } from '@/test/server'

const history = [
  point('2025-12-31', [
    line({ account_id: 'pea', name: 'PEA', kind: 'investment', value: 10_000 }),
    line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 4_000 }),
  ]),
  point('2026-06-01', [
    line({ account_id: 'pea', name: 'PEA', kind: 'investment', value: 12_500 }),
    line({ account_id: 'livret', name: 'Livret A', kind: 'savings', value: 3_000 }),
    line({ account_id: 'pret', name: 'Prêt', kind: 'loan', value: 3_000 }),
  ]),
]

function mockScreen(points = history) {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/history', { points })
}

async function panel(title: string) {
  const section = (await screen.findByRole('heading', { name: title })).closest('section')
  if (!section) throw new Error(`Panneau ${title} introuvable`)
  return within(section)
}

describe('patrimoine à une date', () => {
  it('se lit au 1er janvier en deux clics depuis l’analyse', async () => {
    mockScreen()
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    mockJson('get', '/api/performance', { portfolio: null, accounts: [], benchmark: null })
    const { user } = renderApp('/analyse')
    await user.click(await screen.findByRole('link', { name: 'Patrimoine à une date' }))
    await user.click(await screen.findByRole('button', { name: '1er janvier' }))

    expect(await screen.findByText(/Vous consultez le patrimoine au 1 janvier/)).toBeInTheDocument()
    const totals = await panel('Totaux')
    expect(totals.getByRole('row', { name: /Patrimoine net/ })).toHaveTextContent('6 000')
  })

  it('compare deux dates : écart par catégorie et par compte', async () => {
    mockScreen()
    const { user } = renderApp('/analyse/date')
    fireEvent.change(await screen.findByLabelText('Date'), { target: { value: '2026-01-15' } })
    await user.click(screen.getByLabelText('Comparer avec une autre date'))
    fireEvent.change(screen.getByLabelText('Date 2'), { target: { value: '2026-06-01' } })

    expect(pageText()).toMatch(/Vous comparez le 15 janvier 2026 \(dernier instantané : 31 décembre 2025\) au 1 juin 2026/)
    const totals = await panel('Totaux')
    expect(totals.getByRole('row', { name: /Patrimoine net/ })).toHaveTextContent('+6 500')
    const categories = await panel('Par catégorie')
    expect(categories.getByRole('row', { name: /Épargne/ })).toHaveTextContent('+3 000')
    const accounts = await panel('Par compte')
    expect(accounts.getByRole('row', { name: /Prêt/ })).toHaveTextContent('+1 000')
    // Vue foyer avec deux membres : l'écart de chacun.
    const members = await panel('Par membre')
    expect(members.getByRole('row', { name: /Camille/ })).toHaveTextContent('+6 500')
  })

  it('explique qu’aucun instantané n’existe avant le début de l’historique', async () => {
    mockScreen()
    renderApp('/analyse/date')
    fireEvent.change(await screen.findByLabelText('Date'), { target: { value: '2025-01-01' } })
    expect(screen.getByText('Aucun instantané à cette date')).toBeInTheDocument()
    expect(pageText()).toContain('L’historique commence le 31 décembre 2025')
  })

  it('signale un instantané sans détail par compte', async () => {
    mockScreen([point('2025-12-01', null, { gross_assets: 5_000, liabilities: 0, net_worth: 5_000 })])
    renderApp('/analyse/date')
    expect(await screen.findByText(/Le détail par catégorie et par compte n’existe pas/)).toBeInTheDocument()
  })

  it('affiche l’erreur de l’API', async () => {
    mockShell()
    mockApiError('get', '/api/history', 500, 'Historique indisponible.')
    renderApp('/analyse/date')
    expect(await screen.findByText(/Historique indisponible/)).toBeInTheDocument()
  })
})
