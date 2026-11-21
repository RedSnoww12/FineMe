import { onlineManager } from '@tanstack/react-query'
import { act, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { UNAUTHORIZED_EVENT } from '@/shared/api/client'
import { PERSISTED_CACHE_KEY } from '@/shared/api/persistence'
import { MASK } from '@/shared/lib/format'
import { buildOverview } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { mockJson } from '@/test/server'

function mockOverview() {
  mockShell()
  mockJson('get', '/api/overview', buildOverview())
  mockJson('get', '/api/history', { points: [] })
}

const toggles = () => screen.getAllByRole('button', { name: 'Masquer les montants' })

describe('mode discret', () => {
  it('masque tous les montants d’un clic puis les réaffiche, et mémorise le choix', async () => {
    mockOverview()
    const { user } = renderApp('/')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))

    expect(toggles()[0]).toHaveAttribute('aria-pressed', 'false')
    await user.click(toggles()[0])

    await waitFor(() => expect(pageText()).not.toContain('120 000 €'))
    expect(pageText()).not.toContain('180 000 €')
    expect(pageText()).toContain(`${MASK} €`)
    for (const button of toggles()) expect(button).toHaveAttribute('aria-pressed', 'true')
    expect(localStorage.getItem('fineme.discreet')).toBe('on')

    await user.click(toggles()[0])
    await waitFor(() => expect(pageText()).toContain('120 000 €'))
    expect(localStorage.getItem('fineme.discreet')).toBeNull()
  })

  it('se bascule au clavier avec Alt+Maj+D', async () => {
    mockOverview()
    const { user } = renderApp('/')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))

    await user.keyboard('{Alt>}{Shift>}D{/Shift}{/Alt}')
    await waitFor(() => expect(pageText()).not.toContain('120 000 €'))
    expect(pageText()).toContain(`${MASK} €`)
  })

  it('conserve la saisie d’un formulaire ouvert', async () => {
    mockShell()
    mockJson('get', '/api/accounts', [])
    const { user } = renderApp('/comptes')
    await user.click(await screen.findByRole('button', { name: 'Ajouter un compte' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un compte' }))
    await user.type(await dialog.findByLabelText('Nom du compte'), 'Livret Horizon')

    await user.keyboard('{Alt>}{Shift>}D{/Shift}{/Alt}')
    await waitFor(() => expect(toggles()[0]).toHaveAttribute('aria-pressed', 'true'))
    expect(screen.getByRole('dialog', { name: 'Ajouter un compte' })).toBeInTheDocument()
    expect(dialog.getByLabelText('Nom du compte')).toHaveValue('Livret Horizon')
  })

  it('reste actif au rechargement', async () => {
    localStorage.setItem('fineme.discreet', 'on')
    mockOverview()
    renderApp('/')
    await waitFor(() => expect(pageText()).toContain(`${MASK} €`))
    expect(pageText()).not.toContain('120 000 €')
  })
})

describe('hors ligne', () => {
  afterEach(() => act(() => onlineManager.setOnline(true)))

  it('signale que les données affichées sont une copie et donne leur date', async () => {
    mockOverview()
    renderApp('/')
    await waitFor(() => expect(pageText()).toContain('120 000 €'))

    act(() => onlineManager.setOnline(false))
    expect(await screen.findByText(/^Hors ligne, données du \d{2}\/\d{2} \d{2}:\d{2}$/)).toBeInTheDocument()
    expect(pageText()).toContain('120 000 €')

    act(() => onlineManager.setOnline(true))
    await waitFor(() => expect(screen.queryByText(/^Hors ligne/)).toBeNull())
  })

  it('efface la copie locale des données à la déconnexion', async () => {
    mockOverview()
    renderApp('/', { persist: true })
    await waitFor(() => expect(pageText()).toContain('120 000 €'))
    await waitFor(() => expect(localStorage.getItem(PERSISTED_CACHE_KEY)).toContain('120000'), { timeout: 3_000 })

    mockJson('get', '/api/auth/status', { setup_required: false })
    act(() => void window.dispatchEvent(new Event(UNAUTHORIZED_EVENT)))
    expect(await screen.findByRole('heading', { name: 'Connexion' })).toBeInTheDocument()
    expect(localStorage.getItem(PERSISTED_CACHE_KEY) ?? '').not.toContain('120000')
  })
})
