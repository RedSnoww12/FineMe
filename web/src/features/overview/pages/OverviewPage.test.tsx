import { waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { HistoryPoint } from '@/shared/api/types'
import { buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockJson } from '@/test/server'

function snapshot(date: string, net_worth: number): HistoryPoint {
  return { date, gross_assets: net_worth, liabilities: 0, net_worth, accounts: null }
}

function mockHistory(points: HistoryPoint[]) {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/history', { points })
}

async function historyPanel() {
  const heading = await screen.findByRole('heading', { name: 'Évolution du patrimoine net' })
  const panel = heading.closest('section')
  if (!panel) throw new Error('Panneau d’historique introuvable')
  return panel
}

describe('historique du patrimoine', () => {
  it('signale les jours sans instantané, estimés sur la courbe', async () => {
    mockHistory([snapshot('2026-03-01', 1000), snapshot('2026-03-04', 1300), snapshot('2026-03-05', 1400)])
    renderApp('/')
    expect(await screen.findByText(/2 jours sans instantané sur la période/)).toBeInTheDocument()
  })

  it('n’affiche aucune mention d’estimation quand chaque jour a son instantané', async () => {
    mockHistory([snapshot('2026-03-01', 1000), snapshot('2026-03-02', 1100)])
    renderApp('/')
    const panel = await historyPanel()
    await waitFor(() => expect(panel.querySelector('.recharts-responsive-container')).not.toBeNull())
    expect(pageText(panel)).not.toContain('sans instantané')
    expect(pageText(panel)).not.toContain('se construit au fil du temps')
  })

  it('explique que la courbe se construit tant qu’il n’y a qu’un instantané', async () => {
    mockHistory([snapshot('2026-03-01', 1000)])
    renderApp('/')
    expect(await screen.findByText(/FineMe enregistre chaque jour un point de votre patrimoine/)).toBeInTheDocument()
  })
})
