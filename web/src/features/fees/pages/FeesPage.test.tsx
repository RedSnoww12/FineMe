import { waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { money } from '@/shared/lib/format'
import { buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockJson, mockUnreachable } from '@/test/server'
import { simulateFees, type FeeScenario } from '../lib/fees'

const plain = (text: string) => text.replace(/[  ]/g, ' ')

// Mêmes hypothèses que le premier exemple proposé par l'écran (ETF en PEA).
const firstScenario: FeeScenario = {
  initial: 10_000,
  monthly: 200,
  years: 20,
  grossReturnPct: 6,
  entryFeePct: 0,
  managementFeePct: 0,
  terPct: 0.25,
  brokerage: { mode: 'percent', value: 0.5 },
  performanceFeePct: 0,
}

describe('simulateur de frais', () => {
  it('demande des montants avant de comparer', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    renderApp('/frais')
    expect(
      await screen.findByText('Saisissez un montant initial ou un versement mensuel pour comparer les scénarios.'),
    ).toBeInTheDocument()
    expect(screen.getAllByLabelText('Rendement brut')).toHaveLength(2)
  })

  it('recalcule chaque scénario à la saisie, avec les fonctions pures', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    const { user } = renderApp('/frais')

    await user.type(await screen.findByLabelText('Montant initial'), '10 000')
    await user.type(screen.getByLabelText('Versement mensuel'), '200')

    const expected = simulateFees(firstScenario)
    await waitFor(() => expect(pageText()).toContain(plain(money(expected.finalValue))))
    expect(pageText()).toContain(plain(money(expected.totalFees)))
    expect(pageText()).toContain(plain(money(expected.lostToFees)))
    expect(pageText()).toContain(plain(money(expected.baselineValue)))

    // Des frais courants plus élevés font baisser la valeur finale du premier scénario.
    const ter = screen.getAllByLabelText('Frais courants des fonds')[0]
    await user.clear(ter)
    await user.type(ter, '2')
    const costlier = simulateFees({ ...firstScenario, terPct: 2 })
    expect(costlier.finalValue).toBeLessThan(expected.finalValue)
    await waitFor(() => expect(pageText()).toContain(plain(money(costlier.finalValue))))
  })

  it('ajoute et retire un troisième scénario', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    const { user } = renderApp('/frais')
    await user.click(await screen.findByRole('button', { name: 'Ajouter un troisième scénario' }))
    expect(screen.getAllByLabelText('Rendement brut')).toHaveLength(3)
    expect(screen.queryByRole('button', { name: 'Ajouter un troisième scénario' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Retirer le scénario ETF en assurance-vie' }))
    expect(screen.getAllByLabelText('Rendement brut')).toHaveLength(2)
  })

  it('préremplit le premier scénario avec les frais moyens du portefeuille', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview({ financial_assets: 45_000, weighted_expense_ratio: 0.0031 }))
    const { user } = renderApp('/frais')
    const button = await screen.findByRole('button', { name: 'Préremplir avec mon portefeuille' })
    await waitFor(() => expect(button).toBeEnabled())
    await user.click(button)

    expect(screen.getByLabelText('Montant initial')).toHaveValue('45000')
    expect(screen.getByLabelText('Scénario 1')).toHaveValue('Mon portefeuille')
    expect(screen.getAllByLabelText('Frais courants des fonds')[0]).toHaveValue('0,31')
  })

  it('reste utilisable quand le portefeuille est indisponible', async () => {
    mockShell()
    mockUnreachable('get', '/api/overview')
    const { user } = renderApp('/frais')
    expect(await screen.findByText('Portefeuille indisponible pour le moment.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Préremplir avec mon portefeuille' })).toBeDisabled()
    await user.type(screen.getByLabelText('Montant initial'), '5000')
    const expected = simulateFees({ ...firstScenario, initial: 5_000, monthly: 0 })
    await waitFor(() => expect(pageText()).toContain(plain(money(expected.finalValue))))
  })
})
