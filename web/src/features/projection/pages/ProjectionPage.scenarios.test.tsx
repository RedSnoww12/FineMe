import { within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { buildGoal, buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockApiError, mockJson } from '@/test/server'

type Actor = ReturnType<typeof renderApp>['user']

const thisYear = new Date().getFullYear()

// jsdom ne mesure pas la mise en page : Recharts avertit à chaque recalcul que le graphique fait 0 × 0.
beforeEach(() => {
  const warn = console.warn
  vi.spyOn(console, 'warn').mockImplementation((...args: unknown[]) => {
    if (String(args[0]).includes('of chart should be greater than 0')) return
    warn(...args)
  })
})
afterEach(() => {
  vi.restoreAllMocks()
})

async function openProjection() {
  const view = renderApp('/projection')
  await screen.findByRole('heading', { name: 'Hypothèses' })
  // Patrimoine chargé (ou en échec) : le capital de départ n'est plus prérempli après coup.
  await screen.findAllByText(/^(Actifs financiers|Patrimoine indisponible.*)$/)
  return view
}

function panelOf(heading: HTMLElement) {
  const section = heading.closest('section')
  if (!section) throw new Error('Panneau introuvable')
  return section
}

const hypotheses = () => within(panelOf(screen.getByRole('heading', { name: 'Hypothèses' })))
const resultText = () => pageText(panelOf(screen.getByRole('heading', { name: /^Capital en \d{4}$/ })))

async function replace(actor: Actor, label: string, value: string) {
  const field = hypotheses().getByLabelText(label)
  await actor.clear(field)
  await actor.type(field, value)
}

/** Hypothèses simples à vérifier à la main : 10 000 € placés, sans versement ni inflation. */
async function fillSimpleHypotheses(actor: Actor) {
  await replace(actor, 'Capital de départ', '10000')
  await replace(actor, 'Rendement annuel attendu', '6')
  await replace(actor, 'Inflation annuelle', '0')
  await replace(actor, 'Horizon', '10')
}

describe('projection', () => {
  it('reprend les actifs financiers comme capital de départ', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview({ financial_assets: 45_000, net_worth: 120_000 }))
    mockJson('get', '/api/goals', [])
    const { user: actor } = await openProjection()

    expect(await hypotheses().findByDisplayValue('45000')).toBeInTheDocument()
    await actor.click(hypotheses().getByRole('button', { name: 'Patrimoine net' }))
    expect(hypotheses().getByLabelText('Capital de départ')).toHaveValue('120000')
    expect(screen.getByRole('heading', { name: `Capital en ${thisYear + 20}` })).toBeInTheDocument()
  })

  it('recalcule les scénarios à chaque changement d’hypothèse', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    mockJson('get', '/api/goals', [])
    const { user: actor } = await openProjection()
    await fillSimpleHypotheses(actor)

    // 10 000 € à 4, 6 et 8 % par an pendant 10 ans.
    expect(screen.getByRole('heading', { name: `Capital en ${thisYear + 10}` })).toBeInTheDocument()
    let text = resultText()
    expect(text).toContain('Pessimiste (4,0 %)14 802 €')
    expect(text).toContain('Central (6,0 %)17 908 €')
    expect(text).toContain('Optimiste (8,0 %)21 589 €')
    expect(text).toContain('Total versé10 000 €')

    await replace(actor, 'Épargne mensuelle', '100')
    expect(resultText()).toContain('Total versé22 000 €')

    await replace(actor, 'Épargne mensuelle', '0')
    await replace(actor, 'Inflation annuelle', '2')
    text = resultText()
    expect(text).toContain('Central (6,0 %)14 691 €')
    expect(text).toContain('corrigés d’une inflation de 2,0 % par an')

    await actor.click(screen.getByRole('button', { name: 'Euros courants' }))
    expect(resultText()).toContain('Central (6,0 %)17 908 €')

    const table = within(panelOf(screen.getByRole('heading', { name: 'Année par année' })))
    expect(table.getAllByRole('row')).toHaveLength(12) // en-tête + années 0 à 10
  })

  it('signale une saisie illisible', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    mockJson('get', '/api/goals', [])
    const { user: actor } = await openProjection()
    await replace(actor, 'Épargne mensuelle', 'beaucoup')
    expect(hypotheses().getByRole('alert')).toHaveTextContent('Une valeur n’est pas un nombre : elle est comptée comme 0.')
  })

  it('remplit les hypothèses depuis un objectif', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    mockJson('get', '/api/goals', [buildGoal({ target_date: `${thisYear + 5}-06-30` })])
    const { user: actor } = await openProjection()

    await actor.selectOptions(await hypotheses().findByLabelText('Reprendre un objectif'), 'g-1')
    expect(hypotheses().getByLabelText('Capital de départ')).toHaveValue('10000')
    expect(hypotheses().getByLabelText('Épargne mensuelle')).toHaveValue('400')
    expect(hypotheses().getByLabelText('Rendement annuel attendu')).toHaveValue('4')
    expect(hypotheses().getByLabelText('Horizon')).toHaveValue('5')
    expect(screen.getByRole('heading', { name: `Capital en ${thisYear + 5}` })).toBeInTheDocument()
  })

  it('calcule le capital d’indépendance et l’année où il est atteint', async () => {
    mockShell()
    mockJson('get', '/api/overview', buildOverview())
    mockJson('get', '/api/goals', [])
    const { user: actor } = await openProjection()
    await fillSimpleHypotheses(actor)

    const fire = within(panelOf(screen.getByRole('heading', { name: 'Indépendance financière' })))
    await actor.type(fire.getByLabelText('Dépenses annuelles'), '400')
    const text = pageText(panelOf(screen.getByRole('heading', { name: 'Indépendance financière' })))
    expect(text).toContain('Capital à viser10 000 €33 € par mois')
    expect(text).toContain('PessimisteDéjà atteint')
    expect(text).toContain('CentralDéjà atteint')

    await actor.clear(fire.getByLabelText('Taux de retrait annuel'))
    await actor.type(fire.getByLabelText('Taux de retrait annuel'), '2')
    // Cible de 20 000 € : 10 000 € doublent en 12 ans à 6 %.
    expect(pageText(panelOf(screen.getByRole('heading', { name: 'Indépendance financière' })))).toContain(
      `Central${thisYear + 12}dans 11 ans et 11 mois`,
    )
  })

  it('reste utilisable quand le patrimoine ne peut pas être chargé', async () => {
    mockShell()
    mockApiError('get', '/api/overview', 500, 'Lecture du patrimoine impossible.')
    mockJson('get', '/api/goals', [])
    const { user: actor } = await openProjection()

    expect(await hypotheses().findByText('Patrimoine indisponible : saisissez le capital à la main.')).toBeInTheDocument()
    expect(hypotheses().getByLabelText('Capital de départ')).toHaveValue('')
    await fillSimpleHypotheses(actor)
    expect(resultText()).toContain('Central (6,0 %)17 908 €')
  })
})
