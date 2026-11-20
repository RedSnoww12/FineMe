import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { money, pctFromFraction } from '@/shared/lib/format'
import { buildGoal, buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockJson } from '@/test/server'
import { reachProbability, simulateMonteCarlo, toFanPoints, type MonteCarloInput } from '../lib/monteCarlo'

const plain = (text: string) => text.replace(/[\u00a0\u202f]/g, ' ')

const overview = buildOverview()
const thisYear = new Date().getFullYear()

// Hypothèses par défaut de l'écran, capital repris des actifs financiers de la vue d'ensemble.
const defaults: MonteCarloInput = {
  initial: overview.financial_assets,
  monthly: 0,
  annualReturnPct: 5,
  volatilityPct: 15,
  years: 20,
  simulations: 2000,
  seed: 1,
}

function expectedFinal(input: MonteCarloInput) {
  const result = simulateMonteCarlo(input)
  const points = toFanPoints(result.rows, { initial: input.initial, monthly: input.monthly, inflationPct: 2, real: true, startYear: thisYear })
  return { result, last: points[points.length - 1] }
}

function mockScreen(goals = [buildGoal()]) {
  mockShell()
  mockJson('get', '/api/overview', overview)
  mockJson('get', '/api/goals', goals)
}

describe('projection de Monte-Carlo', () => {
  it('affiche l’éventail des percentiles et invite à saisir une cible', async () => {
    mockScreen([])
    renderApp('/projection')

    const { last } = expectedFinal(defaults)
    await waitFor(() => expect(pageText()).toContain(plain(money(last.p50))))
    expect(pageText()).toContain(plain(money(last.p10)))
    expect(pageText()).toContain(plain(money(last.p90)))
    expect(screen.getByText('Saisissez un capital cible.')).toBeInTheDocument()

    // Alternative accessible : le tableau des percentiles, une ligne par année.
    const table = screen.getByRole('table', { name: 'Capital simulé par année : percentiles 10, 50 et 90' })
    expect(within(table).getAllByRole('row')).toHaveLength(1 + 21)
  })

  it('calcule la probabilité d’atteindre le capital cible à la date visée', async () => {
    mockScreen([])
    const { user } = renderApp('/projection')

    await user.type(await screen.findByLabelText('Capital cible'), '100 000')
    const { result } = expectedFinal(defaults)
    const probability = reachProbability(result, 100_000, { inflationPct: 2, real: true })
    const label = `Probabilité d’atteindre ${money(100_000)} en ${thisYear + 20}`
    const stat = (await screen.findByText(plain(label))).parentElement as HTMLElement
    expect(plain(stat.textContent ?? '')).toContain(plain(pctFromFraction(probability, 0)))

    // Sans volatilité, la simulation rejoint la projection centrale : la cible est atteinte ou non à coup sûr.
    const volatility = screen.getByLabelText('Volatilité annuelle')
    await user.clear(volatility)
    await user.type(volatility, '0')
    await waitFor(() => expect(plain(stat.textContent ?? '')).toMatch(/(100|0) %$/))
  })

  it('relance un tirage avec une autre graine', async () => {
    mockScreen([])
    const { user } = renderApp('/projection')
    const first = expectedFinal(defaults).last
    await waitFor(() => expect(pageText()).toContain(plain(money(first.p50))))

    await user.click(screen.getByRole('button', { name: 'Nouveau tirage' }))
    const second = expectedFinal({ ...defaults, seed: 2 }).last
    await waitFor(() => expect(pageText()).toContain(plain(money(second.p50))))
  })

  it('reprend le capital cible d’un objectif', async () => {
    mockScreen([buildGoal()])
    const { user } = renderApp('/projection')

    await user.selectOptions(await screen.findByLabelText('Reprendre un objectif'), 'g-1')
    expect(screen.getByLabelText('Capital cible')).toHaveValue('40000')
    expect(pageText()).toContain(plain(`Probabilité d’atteindre ${money(40_000)}`))
  })
})
