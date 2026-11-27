import { waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { leaveTo } from '../lib/leave'
import { renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

vi.mock('../lib/leave', () => ({ leaveTo: vi.fn() }))

const ID = `fm_oar_${'a'.repeat(43)}`
const PATH = `/api/oauth/requests/${ID}`
const ROUTE = `/oauth/consent?request=${ID}`
const REQUEST = { client_name: 'Claude', redirect_host: 'claude.ai', scope: 'read', expires_in: 600 }

const open = (route = ROUTE) => renderApp(route, { authenticated: false })

describe('consentement OAuth', () => {
  beforeEach(() => vi.mocked(leaveTo).mockClear())

  it('signale le chargement de la demande', async () => {
    server.use(http.get(`${API}${PATH}`, () => new Promise<never>(() => {})))
    open()
    expect(await screen.findByRole('status', { name: 'Chargement de la demande…' })).toBeInTheDocument()
  })

  it('montre le demandeur, l’adresse de retour et la portée, sans session', async () => {
    mockJson('get', PATH, REQUEST)
    open()
    expect(await screen.findByRole('heading', { name: 'Autoriser « Claude » ?' })).toBeInTheDocument()
    expect(screen.getByText('claude.ai')).toBeInTheDocument()
    expect(screen.getByText(/Lecture seule de tout le patrimoine/)).toBeInTheDocument()
    expect(screen.getByText(/expire à \d{2}:\d{2}/)).toBeInTheDocument()
    expect(screen.getByLabelText('Mot de passe')).toHaveAttribute('type', 'password')
  })

  it('n’envoie rien sans mot de passe', async () => {
    mockJson('get', PATH, REQUEST)
    const calls = spyOn('post', `${PATH}/approve`)
    const { user } = open()
    await user.click(await screen.findByRole('button', { name: 'Autoriser' }))
    expect(screen.getByText('Saisissez votre mot de passe.')).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('autorise avec le mot de passe et le code puis renvoie vers le client', async () => {
    mockJson('get', PATH, REQUEST)
    let received: unknown
    server.use(
      http.post(`${API}${PATH}/approve`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ redirect_to: 'https://claude.ai/retour?code=abc' })
      }),
    )
    const { user } = open()
    await user.type(await screen.findByLabelText('Mot de passe'), 'tournesol-violet-42')
    await user.type(screen.getByLabelText('Code de vérification'), '123 456')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))

    await waitFor(() => expect(leaveTo).toHaveBeenCalledWith('https://claude.ai/retour?code=abc'))
    expect(received).toEqual({ password: 'tournesol-violet-42', code: '123456' })
    expect(screen.getByRole('button', { name: 'Autoriser' })).toBeDisabled()
    expect(screen.getByLabelText('Mot de passe')).toHaveValue('')
    expect(screen.getByLabelText('Code de vérification')).toHaveValue('')
  })

  it('ne part pas vers un autre hôte que celui montré', async () => {
    mockJson('get', PATH, REQUEST)
    mockJson('post', `${PATH}/approve`, { redirect_to: 'https://ailleurs.example.test/retour?code=abc' })
    const { user } = open()
    await user.type(await screen.findByLabelText('Mot de passe'), 'tournesol-violet-42')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))
    expect(await screen.findByText(/Adresse de retour inattendue/)).toBeInTheDocument()
    expect(leaveTo).not.toHaveBeenCalled()
  })

  it('annonce la demande expirée quand elle disparaît au moment d’autoriser', async () => {
    mockJson('get', PATH, REQUEST)
    mockApiError('post', `${PATH}/approve`, 404, 'Demande introuvable.')
    const { user } = open()
    await user.type(await screen.findByLabelText('Mot de passe'), 'tournesol-violet-42')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))
    expect(await screen.findByRole('heading', { name: 'Demande introuvable ou expirée' })).toBeInTheDocument()
  })

  it('affiche l’échec d’un refus et le plafond d’autorisations atteint', async () => {
    mockJson('get', PATH, REQUEST)
    mockApiError('post', `${PATH}/deny`, 500, 'Refus impossible.')
    mockApiError('post', `${PATH}/approve`, 409, 'Déjà 10 autorisations actives.')
    const { user } = open()
    await user.click(await screen.findByRole('button', { name: 'Refuser' }))
    expect(await screen.findByText('Refus impossible.')).toBeInTheDocument()
    await user.type(screen.getByLabelText('Mot de passe'), 'tournesol-violet-42')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))
    expect(await screen.findByText('Déjà 10 autorisations actives.')).toBeInTheDocument()
    expect(leaveTo).not.toHaveBeenCalled()
  })

  it('affiche le refus de l’API et reste sur l’écran', async () => {
    mockJson('get', PATH, REQUEST)
    mockApiError('post', `${PATH}/approve`, 403, 'Mot de passe incorrect.')
    const { user } = open()
    await user.type(await screen.findByLabelText('Mot de passe'), 'faux-mot-de-passe')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))
    expect(await screen.findByText('Mot de passe incorrect.')).toBeInTheDocument()
    expect(leaveTo).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Autoriser' })).toBeEnabled()
  })

  it('attend le délai imposé après trop de tentatives', async () => {
    mockJson('get', PATH, REQUEST)
    server.use(
      http.post(`${API}${PATH}/approve`, () =>
        HttpResponse.json({ error: 'Trop de tentatives.' }, { status: 429, headers: { 'Retry-After': '30' } }),
      ),
    )
    const { user } = open()
    await user.type(await screen.findByLabelText('Mot de passe'), 'faux-mot-de-passe')
    await user.click(screen.getByRole('button', { name: 'Autoriser' }))
    expect(await screen.findByRole('button', { name: /Réessayer dans/ })).toBeDisabled()
  })

  it('refuse sans mot de passe et renvoie vers le client', async () => {
    mockJson('get', PATH, REQUEST)
    mockJson('post', `${PATH}/deny`, { redirect_to: 'https://claude.ai/retour?error=access_denied' })
    const { user } = open()
    await user.click(await screen.findByRole('button', { name: 'Refuser' }))
    await waitFor(() => expect(leaveTo).toHaveBeenCalledWith('https://claude.ai/retour?error=access_denied'))
  })

  it('explique qu’une demande inconnue ou expirée doit être relancée', async () => {
    mockApiError('get', PATH, 404, 'Demande introuvable.')
    open()
    expect(await screen.findByRole('heading', { name: 'Demande introuvable ou expirée' })).toBeInTheDocument()
    expect(screen.queryByLabelText('Mot de passe')).not.toBeInTheDocument()
  })

  it('traite une adresse sans identifiant comme une demande introuvable', async () => {
    open('/oauth/consent')
    expect(await screen.findByRole('heading', { name: 'Demande introuvable ou expirée' })).toBeInTheDocument()
  })

  it('n’envoie rien pour un identifiant mal formé', async () => {
    open('/oauth/consent?request=..')
    expect(await screen.findByRole('heading', { name: 'Demande introuvable ou expirée' })).toBeInTheDocument()
  })

  it('propose de réessayer quand le serveur échoue', async () => {
    mockApiError('get', PATH, 500, 'Erreur interne.')
    const { user } = open()
    expect(await screen.findByRole('alert')).toHaveTextContent('Erreur interne.')
    mockJson('get', PATH, REQUEST)
    await user.click(screen.getByRole('button', { name: 'Réessayer' }))
    expect(await screen.findByRole('heading', { name: 'Autoriser « Claude » ?' })).toBeInTheDocument()
  })
})
