import { fireEvent, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { HistoryComparison } from '@/shared/api/types'
import { buildHistoryPoint as point, buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

/** Comparaison fictive calculée par l'API : 31 décembre 2025 → 1er juin 2026. */
function comparison(a: string, b: string): HistoryComparison {
  const single = a === b
  return {
    first_date: '2025-12-31',
    a: { requested: a, date: '2025-12-31' },
    b: { requested: b, date: single ? '2025-12-31' : '2026-06-01' },
    totals: [
      {
        key: 'gross_assets',
        from: 10_000,
        to: single ? 10_000 : 15_500,
        delta: single ? 0 : 5_500,
      },
      {
        key: 'liabilities',
        from: 4_000,
        to: single ? 4_000 : 3_000,
        delta: single ? 0 : -1_000,
      },
      {
        key: 'net_worth',
        from: 6_000,
        to: single ? 6_000 : 12_500,
        delta: single ? 0 : 6_500,
      },
    ],
    categories: [
      { key: 'savings', from: 0, to: 3_000, delta: 3_000 },
      { key: 'investment', from: 10_000, to: 12_500, delta: 2_500 },
    ],
    accounts: [
      {
        key: 'livret',
        name: 'Livret A',
        kind: 'savings',
        from: 0,
        to: 3_000,
        delta: 3_000,
      },
      {
        key: 'pea',
        name: 'PEA',
        kind: 'investment',
        from: 10_000,
        to: 12_500,
        delta: 2_500,
      },
      {
        key: 'pret',
        name: 'Prêt',
        kind: 'loan',
        from: -4_000,
        to: -3_000,
        delta: 1_000,
      },
    ],
    members: [
      {
        key: 'm-camille',
        name: 'Camille',
        from: 6_000,
        to: 12_500,
        delta: 6_500,
      },
      { key: 'm-sacha', name: 'Sacha', from: 0, to: 0, delta: 0 },
    ],
  }
}

/** Répond à `/api/history/compare` selon les dates demandées ; `received` garde les paramètres reçus. */
function mockCompare(build: (a: string, b: string) => HistoryComparison = comparison) {
  const received: { a: string; b: string; member_id: string | null }[] = []
  server.use(
    http.get(`${API}/api/history/compare`, ({ request }) => {
      const params = new URL(request.url).searchParams
      const a = params.get('a') ?? ''
      const b = params.get('b') ?? ''
      received.push({ a, b, member_id: params.get('member_id') })
      return HttpResponse.json(build(a, b))
    }),
  )
  return received
}

async function panel(title: string) {
  const section = (await screen.findByRole('heading', { name: title })).closest('section')
  if (!section) throw new Error(`Panneau ${title} introuvable`)
  return within(section)
}

describe('patrimoine à une date', () => {
  it('se lit au 1er janvier en deux clics depuis l’analyse', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    mockJson('get', '/api/history', { points: [point('2025-12-31', null)] })
    mockJson('get', '/api/accounts', [])
    mockJson('get', '/api/goals', [])
    mockJson('get', '/api/performance', {
      portfolio: null,
      accounts: [],
      benchmark: null,
    })
    const received = mockCompare()
    const { user } = renderApp('/analyse')
    await user.click(await screen.findByRole('link', { name: 'Patrimoine à une date' }))
    await user.click(await screen.findByRole('button', { name: '1er janvier' }))

    expect(await screen.findByText(/Vous consultez le patrimoine au 1 janvier/)).toBeInTheDocument()
    const totals = await panel('Totaux')
    expect(totals.getByRole('row', { name: /Patrimoine net/ })).toHaveTextContent('6 000')
    // Une seule date : la même des deux côtés.
    const last = received.at(-1)
    expect(last?.a).toBe(last?.b)
  })

  it('compare deux dates : écart par catégorie, par compte et par membre', async () => {
    mockShell()
    const received = mockCompare()
    const { user } = renderApp('/analyse/date')
    fireEvent.change(await screen.findByLabelText('Date'), {
      target: { value: '2026-01-15' },
    })
    await user.click(screen.getByLabelText('Comparer avec une autre date'))
    fireEvent.change(screen.getByLabelText('Date 2'), {
      target: { value: '2026-06-01' },
    })

    expect(
      await screen.findByText(
        /Vous comparez le 15 janvier 2026 \(dernier instantané : 31 décembre 2025\) au 1 juin 2026\./,
      ),
    ).toBeInTheDocument()
    expect(received.at(-1)).toEqual({
      a: '2026-01-15',
      b: '2026-06-01',
      member_id: null,
    })
    const totals = await panel('Totaux')
    expect(totals.getByRole('row', { name: /Patrimoine net/ })).toHaveTextContent('+6 500')
    const categories = await panel('Par catégorie')
    expect(categories.getByRole('row', { name: /Épargne/ })).toHaveTextContent('+3 000')
    const accounts = await panel('Par compte')
    expect(accounts.getByRole('row', { name: /Prêt/ })).toHaveTextContent('+1 000')
    const members = await panel('Par membre')
    expect(members.getByRole('row', { name: /Camille/ })).toHaveTextContent('+6 500')
  })

  it('explique qu’aucun instantané n’existe avant le début de l’historique', async () => {
    mockShell()
    mockCompare((a, b) => ({
      first_date: '2025-12-31',
      a: null,
      b: a === b ? null : { requested: b, date: '2025-12-31' },
      totals: null,
      categories: null,
      accounts: null,
      members: null,
    }))
    renderApp('/analyse/date')
    fireEvent.change(await screen.findByLabelText('Date'), {
      target: { value: '2025-01-01' },
    })
    expect(await screen.findByText('Aucun instantané à cette date')).toBeInTheDocument()
    expect(pageText()).toContain('L’historique commence le 31 décembre 2025')
  })

  it('signale un instantané sans détail par compte', async () => {
    mockShell()
    mockCompare((a, b) => ({
      ...comparison(a, b),
      categories: null,
      accounts: null,
    }))
    renderApp('/analyse/date')
    expect(await screen.findByText(/Le détail par catégorie et par compte n’existe pas/)).toBeInTheDocument()
  })

  it('affiche l’erreur de l’API sans masquer la saisie des dates', async () => {
    mockShell()
    mockApiError('get', '/api/history/compare', 500, 'Historique indisponible.')
    renderApp('/analyse/date')
    expect(await screen.findByText(/Historique indisponible/)).toBeInTheDocument()
    expect(screen.getByLabelText('Date')).toBeInTheDocument()
  })

  it('n’interroge pas l’API tant qu’une date est incomplète', async () => {
    mockShell()
    const received = mockCompare()
    renderApp('/analyse/date')
    const input = await screen.findByLabelText('Date')
    await screen.findByRole('heading', { name: 'Totaux' })
    const before = received.length
    fireEvent.change(input, { target: { value: '' } })
    expect(await screen.findByText(/Indiquez une date complète/)).toBeInTheDocument()
    expect(received).toHaveLength(before)
    fireEvent.change(input, { target: { value: '2026-03-01' } })
    expect(await screen.findByRole('heading', { name: 'Totaux' })).toBeInTheDocument()
    expect(received.at(-1)?.a).toBe('2026-03-01')
  })
})
