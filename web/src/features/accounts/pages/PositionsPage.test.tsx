import { within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { buildAccount, buildAccountDetail, buildHolding, camille } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { renderApp, screen } from '@/test/render'
import { mockApiError, mockJson } from '@/test/server'

const solo = [{ member_id: camille.id, name: camille.name, share: 1 }]
const etfPea = buildHolding({ id: 'h-pea', account_id: 'pea' })
const etfCto = buildHolding({ id: 'h-cto', account_id: 'cto', quantity: 50, buy_price: 360, value: 16_500, invested: 18_000, gain: -1_500 })
const pea = buildAccountDetail({ id: 'pea', name: 'PEA Camille', kind: 'investment', owners: solo, holdings: [etfPea] })
const cto = buildAccountDetail({ id: 'cto', name: 'CTO joint', kind: 'investment', holdings: [etfCto] })
const house = buildAccount({ id: 'maison', name: 'Maison', kind: 'real_estate' })

function mockScreen() {
  mockShell()
  mockJson('get', '/api/accounts', [pea, cto, house])
  mockJson('get', '/api/accounts/pea', pea)
  mockJson('get', '/api/accounts/cto', cto)
  mockJson('get', '/api/accounts/pea/operations', [
    { id: 'o-1', account_id: 'pea', kind: 'buy', date: '2025-02-03', holding_id: 'h-pea', holding_name: etfPea.name, quantity: 100, unit_price: 300, amount: 30_000, fees: 0, currency: 'EUR', note: null, realized_gain: null },
    { id: 'o-2', account_id: 'pea', kind: 'deposit', date: '2025-02-01', holding_id: null, holding_name: null, quantity: null, unit_price: null, amount: 30_000, fees: 0, currency: 'EUR', note: null, realized_gain: null },
  ])
  mockJson('get', '/api/accounts/cto/operations', [
    { id: 'o-3', account_id: 'cto', kind: 'dividend', date: '2025-06-10', holding_id: 'h-cto', holding_name: etfCto.name, quantity: null, unit_price: null, amount: 42, fees: 0, currency: 'EUR', note: null, realized_gain: null },
  ])
  mockJson('get', '/api/holdings/h-pea/prices', {
    points: [
      { date: '2025-03-01', price: 320, currency: 'EUR' },
      { date: '2025-03-02', price: 330, currency: 'EUR' },
    ],
  })
}

describe('positions consolidées', () => {
  it('affiche un ETF détenu dans deux comptes sur une seule ligne cliquable', async () => {
    mockScreen()
    const { user } = renderApp('/comptes')
    await user.click(await screen.findByRole('link', { name: 'Positions consolidées' }))

    const links = await screen.findAllByRole('link', { name: 'Fonds Indice Monde' })
    expect(links).toHaveLength(1)
    const row = links[0]?.closest('tr')
    if (!row) throw new Error('Ligne introuvable')
    // 33 000 + 16 500 = 49 500 € pour 150 parts dans 2 comptes.
    expect(row).toHaveTextContent('49 500')
    expect(row).toHaveTextContent('150')
  })

  it('détaille l’actif : PRU moyen pondéré, répartition par compte et par membre, opérations de tous les comptes', async () => {
    mockScreen()
    renderApp(`/comptes/positions/${encodeURIComponent('isin:FR0000000001')}`)

    expect(await screen.findByRole('heading', { name: 'Fonds Indice Monde' })).toBeInTheDocument()
    expect(screen.getByText('320,00 €')).toBeInTheDocument()

    const byAccount = within((await screen.findByRole('heading', { name: 'Par compte' })).closest('section') as HTMLElement)
    expect(byAccount.getByRole('row', { name: /PEA Camille/ })).toHaveTextContent('67 %')
    const byMember = within((await screen.findByRole('heading', { name: 'Par membre' })).closest('section') as HTMLElement)
    expect(byMember.getByRole('row', { name: /Camille/ })).toHaveTextContent('41 250')

    const operations = within((await screen.findByRole('heading', { name: /Opérations/ })).closest('section') as HTMLElement)
    expect(await operations.findByText('Dividende')).toBeInTheDocument()
    expect(operations.getByText('Achat')).toBeInTheDocument()
    // Un versement ne concerne pas l'actif.
    expect(operations.queryByText('Versement')).toBeNull()
    expect(await screen.findByRole('img', { name: /Cours de Fonds Indice Monde/ })).toBeInTheDocument()
  })

  it('ouvre la vue consolidée depuis la fiche d’un compte', async () => {
    mockScreen()
    const { user } = renderApp('/comptes/pea')
    await user.click(await screen.findByRole('link', { name: 'Fonds Indice Monde' }))
    expect(await screen.findByText('PRU moyen pondéré')).toBeInTheDocument()
  })

  it('signale un actif inconnu', async () => {
    mockScreen()
    renderApp('/comptes/positions/isin%3AXX')
    expect(await screen.findByText('Position introuvable')).toBeInTheDocument()
  })

  it('affiche l’erreur de l’API', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [pea])
    mockApiError('get', '/api/accounts/pea', 500, 'Base indisponible.')
    renderApp('/comptes/positions')
    expect(await screen.findByText(/Base indisponible/)).toBeInTheDocument()
  })
})
