import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Performance, PerformanceReport } from '@/shared/api/types'
import { buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

const measured: Performance = {
  from: '2024-12-31',
  to: '2026-01-31',
  start_value: 10_000,
  end_value: 13_000,
  net_flows: 1000,
  gain: 2000,
  twr: 0.18,
  twr_annualized: 0.165,
  mwr: 0.17,
  mwr_annualized: 0.157,
  years: [
    { year: 2025, from: '2024-12-31', to: '2025-12-31', twr: 0.12 },
    { year: 2026, from: '2025-12-31', to: '2026-01-31', twr: 0.054 },
  ],
  points: [
    { date: '2024-12-31', value: 100, benchmark: null },
    { date: '2025-12-31', value: 112, benchmark: 112 },
    { date: '2026-01-31', value: 118, benchmark: 115 },
  ],
}

const report: PerformanceReport = {
  portfolio: measured,
  accounts: [
    { account_id: 'pea', name: 'PEA Camille', kind: 'investment', performance: measured },
    {
      account_id: 'crypto',
      name: 'Portefeuille crypto',
      kind: 'crypto',
      performance: { ...measured, twr: null, twr_annualized: null, mwr: null, mwr_annualized: null, years: [], points: [] },
    },
  ],
  benchmark: {
    name: 'ETF Monde démo',
    symbol: 'MONDE',
    isin: null,
    kind: 'security',
    currency: 'EUR',
    prices: 2,
    first_date: '2025-12-30',
    last_date: '2026-01-31',
    last_error: null,
    comparison: { from: '2025-12-31', to: '2026-01-31', benchmark_return: 0.0268, portfolio_return: 0.0536 },
    message: 'L’indice n’a d’historique qu’à partir du 31/12/2025 : la comparaison commence à cette date.',
  },
}

function mockScreen(performance: PerformanceReport | null) {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/accounts', [])
  mockJson('get', '/api/goals', [])
  mockJson('get', '/api/history', { points: [] })
  mockJson('get', '/api/real-estate', { properties: [], totals: { value: 0, remaining_debt: 0, equity: 0, monthly_rent: 0, monthly_payments: 0 } })
  if (performance) mockJson('get', '/api/performance', performance)
}

async function returnsPanel() {
  const heading = await screen.findByRole('heading', { name: 'Rendement des placements' })
  const section = heading.closest('section')
  if (!section) throw new Error('Panneau introuvable')
  return section
}

describe('rendement des placements', () => {
  it('présente TWR, MWR et annualisés, chacun expliqué, les années et la comparaison à l’indice', async () => {
    mockScreen(report)
    const { user } = renderApp('/analyse')
    const panel = await returnsPanel()
    await waitFor(() => expect(pageText(panel)).toContain('Performance des placements+18,0 %'))
    const text = pageText(panel)
    expect(text).toContain('Rendement de votre argent+17,0 %')
    expect(text).toContain('Par an, en moyenne+16,5 %')
    expect(text).toContain('sans tenir compte du montant ni du moment de vos versements')
    expect(text).toContain('l’indice a fait +2,7 % et vos placements +5,4 %')
    expect(text).toContain('la comparaison commence à cette date')

    const years = within(panel).getByRole('table', { name: 'Performance des placements par année civile' })
    expect(pageText(years)).toContain('2026 (partielle)')
    expect(pageText(years)).toContain('+12,0 %')

    expect(within(panel).getByRole('img', { name: /Base 100.*vos placements 118.*indice 115/ })).toBeInTheDocument()
    await user.click(within(panel).getByText('Voir les données en tableau'))
    const curve = within(panel).getByRole('table', { name: 'Évolution de 100 € placés' })
    expect(pageText(curve)).toContain('ETF Monde démo')
    expect(pageText(curve)).toContain('118115')
  })

  it('dit pourquoi une mesure est indéfinie, compte par compte', async () => {
    mockScreen(report)
    const { user } = renderApp('/analyse')
    const panel = await returnsPanel()
    await user.selectOptions(await within(panel).findByLabelText('Mesurer'), 'crypto')
    await waitFor(() => expect(pageText(panel)).toContain('Performance des placementsIndéfini'))
    expect(pageText(panel)).toContain('il faut au moins deux relevés')
    expect(pageText(panel)).toContain('Affiché à partir d’un an d’historique')
  })

  it('choisit un indice après avoir vérifié le formulaire', async () => {
    mockScreen({ ...report, benchmark: null })
    const sent: unknown[] = []
    server.use(
      http.put(`${API}/api/performance/benchmark`, async ({ request }) => {
        sent.push(await request.json())
        return HttpResponse.json({ ...report.benchmark, prices: 0, first_date: null, last_date: null, last_error: 'cours introuvable' })
      }),
    )
    const { user } = renderApp('/analyse')
    const panel = await returnsPanel()
    await user.click(await within(panel).findByRole('button', { name: 'Choisir un indice' }))
    await user.type(within(panel).getByLabelText('Nom de l’indice'), 'ETF Monde démo')
    await user.click(within(panel).getByRole('button', { name: 'Enregistrer l’indice' }))
    expect(await within(panel).findByRole('alert')).toHaveTextContent('Indiquez un ticker ou un ISIN')
    expect(sent).toHaveLength(0)

    await user.type(within(panel).getByLabelText('ISIN'), 'ie00demo0001')
    await user.click(within(panel).getByRole('button', { name: 'Enregistrer l’indice' }))
    await waitFor(() => expect(sent).toEqual([{ name: 'ETF Monde démo', isin: 'IE00DEMO0001', kind: 'security', currency: 'EUR' }]))
  })

  it('gère l’historique vide et l’erreur de chargement', async () => {
    mockScreen({ portfolio: null, accounts: [], benchmark: null })
    renderApp('/analyse')
    const panel = await returnsPanel()
    await waitFor(() => expect(pageText(panel)).toContain('Pas encore assez d’historique'))
    expect(within(panel).getByRole('button', { name: 'Choisir un indice' })).toBeInTheDocument()
  })

  it('affiche une erreur de l’API avec un bouton pour réessayer', async () => {
    mockScreen(null)
    mockApiError('get', '/api/performance', 500, 'Performance indisponible.')
    renderApp('/analyse')
    const panel = await returnsPanel()
    expect(await within(panel).findByText('Performance indisponible.')).toBeInTheDocument()
    expect(within(panel).getByRole('button', { name: /Réessayer/ })).toBeInTheDocument()
  })
})
