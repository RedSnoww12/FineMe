import { waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Bundle, LoginEvent, Member } from '@/shared/api/types'
import { camille, members, sacha, user } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server, spyOn } from '@/test/server'

const CURRENT = 'ancien-mot-de-passe'
const NEXT = 'nouveau-mot-de-passe'

async function openSettings() {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  mockJson('get', '/api/alerts/rules', [])
  const view = renderApp('/parametres')
  await screen.findByRole('heading', { name: 'Mot de passe' })
  return view
}

async function fillPasswordForm(actor: ReturnType<typeof renderApp>['user'], current: string, next: string, confirm = next) {
  await actor.type(screen.getByLabelText('Mot de passe actuel'), current)
  await actor.type(screen.getByLabelText('Nouveau mot de passe'), next)
  await actor.type(screen.getByLabelText('Confirmez le nouveau mot de passe'), confirm)
  await actor.click(screen.getByRole('button', { name: 'Changer le mot de passe' }))
}

describe('changement du mot de passe', () => {
  it('valide la saisie avant tout envoi', async () => {
    const calls = spyOn('put', '/api/auth/password')
    const { user: actor } = await openSettings()

    await actor.click(screen.getByRole('button', { name: 'Changer le mot de passe' }))
    expect(screen.getByText('Saisissez votre mot de passe actuel.')).toBeInTheDocument()
    expect(screen.getByText('Le mot de passe doit contenir au moins 10 caractères.')).toBeInTheDocument()

    await fillPasswordForm(actor, CURRENT, CURRENT)
    expect(screen.getByText('Le nouveau mot de passe doit être différent de l’actuel.')).toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('envoie l’ancien et le nouveau mot de passe puis garde l’appareil connecté avec le nouveau jeton', async () => {
    let received: unknown
    let authorization: string | null = null
    server.use(
      http.put(`${API}/api/auth/password`, async ({ request }) => {
        received = await request.json()
        authorization = request.headers.get('authorization')
        return HttpResponse.json({ token: 'jeton-renouvele', user })
      }),
    )
    const { user: actor } = await openSettings()
    await fillPasswordForm(actor, CURRENT, NEXT)

    expect(await screen.findByText('Mot de passe modifié. Les autres appareils ont été déconnectés.')).toBeInTheDocument()
    expect(received).toEqual({ current_password: CURRENT, new_password: NEXT })
    expect(authorization).toBe('Bearer jeton-de-test')
    expect(localStorage.getItem('fineme.token')).toBe('jeton-renouvele')
    expect(screen.getByLabelText('Mot de passe actuel')).toHaveValue('')
    // Toujours sur l'écran : la session n'a pas été fermée.
    expect(screen.getByRole('heading', { name: 'Paramètres' })).toBeInTheDocument()
  })

  it('affiche le refus de l’API sans déconnecter', async () => {
    mockApiError('put', '/api/auth/password', 403, 'Mot de passe actuel incorrect')
    const { user: actor } = await openSettings()
    await fillPasswordForm(actor, 'mauvais-mot-de-passe', NEXT)

    expect(await screen.findByText('Mot de passe actuel incorrect')).toBeInTheDocument()
    expect(localStorage.getItem('fineme.token')).toBe('jeton-de-test')
    expect(screen.getByRole('heading', { name: 'Paramètres' })).toBeInTheDocument()
  })
})

/** Membres servis depuis un état modifiable : le shell et l'écran relisent la même liste. */
function serveMembers(initial: Member[] = members) {
  const state = { members: [...initial] }
  server.use(http.get(`${API}/api/members`, () => HttpResponse.json(state.members)))
  return state
}

async function openMembers() {
  mockJson('get', '/api/auth/logins', [])
  const view = renderApp('/parametres')
  await screen.findByRole('heading', { name: 'Membres du foyer' })
  await screen.findByRole('button', { name: 'Modifier Sacha' })
  return view
}

describe('membres du foyer', () => {
  it('liste les membres et protège le membre principal', async () => {
    serveMembers()
    await openMembers()
    expect(screen.getByText('Principal')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Supprimer Camille' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Supprimer Sacha' })).toBeEnabled()
  })

  it('ajoute un membre avec la première couleur libre', async () => {
    const state = serveMembers()
    let received: unknown
    server.use(
      http.post(`${API}/api/members`, async ({ request }) => {
        received = await request.json()
        const created: Member = { id: 'm-alix', name: 'Alix', color: '#1E8C82', is_primary: false }
        state.members.push(created)
        return HttpResponse.json(created)
      }),
    )
    const { user: actor } = await openMembers()
    await actor.click(screen.getByRole('button', { name: 'Ajouter un membre' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un membre' }))

    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(dialog.getByText('Indiquez le prénom ou le nom du membre.')).toBeInTheDocument()

    // Les deux premières couleurs sont prises par Camille et Sacha.
    expect(dialog.getByRole('radio', { name: 'Couleur #1E8C82' })).toBeChecked()
    await actor.type(dialog.getByLabelText('Nom'), '  Alix ')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({ name: 'Alix', color: '#1E8C82' })
    expect(await screen.findByRole('button', { name: 'Modifier Alix' })).toBeInTheDocument()
  })

  it('modifie le nom et la couleur d’un membre', async () => {
    const state = serveMembers()
    let received: unknown
    server.use(
      http.put(`${API}/api/members/${sacha.id}`, async ({ request }) => {
        received = await request.json()
        const updated: Member = { ...sacha, name: 'Sacha M.', color: '#8A4B9C' }
        state.members = [camille, updated]
        return HttpResponse.json(updated)
      }),
    )
    const { user: actor } = await openMembers()
    await actor.click(screen.getByRole('button', { name: 'Modifier Sacha' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Modifier le membre' }))
    expect(dialog.getByLabelText('Nom')).toHaveValue('Sacha')
    expect(dialog.getByRole('radio', { name: `Couleur ${sacha.color}` })).toBeChecked()

    await actor.clear(dialog.getByLabelText('Nom'))
    await actor.type(dialog.getByLabelText('Nom'), 'Sacha M.')
    await actor.click(dialog.getByRole('radio', { name: 'Couleur #8A4B9C' }))
    await actor.click(dialog.getByRole('button', { name: 'Enregistrer' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({ name: 'Sacha M.', color: '#8A4B9C' })
    expect(await screen.findByRole('button', { name: 'Modifier Sacha M.' })).toBeInTheDocument()
  })

  it('supprime un membre après confirmation', async () => {
    const state = serveMembers()
    server.use(
      http.delete(`${API}/api/members/${sacha.id}`, () => {
        state.members = [camille]
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const { user: actor } = await openMembers()
    await actor.click(screen.getByRole('button', { name: 'Supprimer Sacha' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Supprimer ce membre ?' }))
    await actor.click(dialog.getByRole('button', { name: 'Supprimer le membre' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Modifier Sacha' })).not.toBeInTheDocument())
  })

  it('explique le refus de l’API quand le membre possède encore une part', async () => {
    serveMembers()
    mockApiError('delete', `/api/members/${sacha.id}`, 409, 'Ce membre détient encore une part d’un compte.')
    const { user: actor } = await openMembers()
    await actor.click(screen.getByRole('button', { name: 'Supprimer Sacha' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Supprimer ce membre ?' }))
    await actor.click(dialog.getByRole('button', { name: 'Supprimer le membre' }))

    expect(await dialog.findByText('Ce membre détient encore une part d’un compte.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Modifier Sacha' })).toBeInTheDocument()
  })

  it('affiche le refus de l’API à l’ajout sans fermer le formulaire', async () => {
    serveMembers()
    mockApiError('post', '/api/members', 409, 'Un membre porte déjà ce nom.')
    const { user: actor } = await openMembers()
    await actor.click(screen.getByRole('button', { name: 'Ajouter un membre' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Ajouter un membre' }))
    await actor.type(dialog.getByLabelText('Nom'), 'Sacha')
    await actor.click(dialog.getByRole('button', { name: 'Ajouter' }))
    expect(await dialog.findByText('Un membre porte déjà ce nom.')).toBeInTheDocument()
  })
})

const bundle: Bundle = {
  version: 1,
  members: [{ name: 'Camille', color: '#3F51B5', is_primary: true }],
  accounts: [
    { name: 'Compte joint', kind: 'checking' },
    { name: 'PEA Camille', kind: 'investment' },
  ],
  goals: [],
}

let restoreDownload = () => {}
afterEach(() => {
  restoreDownload()
  restoreDownload = () => {}
  vi.restoreAllMocks()
})

/** jsdom ne sait ni créer d'URL de Blob ni télécharger : on capte le fichier proposé. */
function captureDownload() {
  const original = { create: URL.createObjectURL, revoke: URL.revokeObjectURL }
  const captured: { blob?: Blob; fileName?: string } = {}
  URL.createObjectURL = (blob: Blob | MediaSource) => {
    captured.blob = blob as Blob
    return 'blob:fineme-export'
  }
  URL.revokeObjectURL = () => {}
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
    captured.fileName = this.download
  })
  restoreDownload = () => {
    URL.createObjectURL = original.create
    URL.revokeObjectURL = original.revoke
  }
  return captured
}

async function openData() {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  mockJson('get', '/api/alerts/rules', [])
  const view = renderApp('/parametres')
  await screen.findByRole('heading', { name: 'Vos données' })
  return view
}

function jsonFile(name: string, content: string) {
  return new File([content], name, { type: 'application/json' })
}

const importInput = () => screen.getByLabelText('Fichier d’export FineMe à importer')

describe('export des données', () => {
  it('télécharge le lot renvoyé par l’API dans un fichier JSON daté', async () => {
    mockJson('get', '/api/export', bundle)
    const captured = captureDownload()
    const { user: actor } = await openData()
    await actor.click(screen.getByRole('button', { name: 'Exporter mes données' }))

    await waitFor(() => expect(captured.fileName).toMatch(/^fineme-export-\d{4}-\d{2}-\d{2}\.json$/))
    expect(JSON.parse(await (captured.blob as Blob).text())).toEqual(bundle)
  })

  it('affiche l’erreur de l’API', async () => {
    mockApiError('get', '/api/export', 500, 'Export impossible pour le moment.')
    const captured = captureDownload()
    const { user: actor } = await openData()
    await actor.click(screen.getByRole('button', { name: 'Exporter mes données' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Export impossible pour le moment.')
    expect(captured.fileName).toBeUndefined()
  })
})

describe('import des données', () => {
  it('résume le fichier, l’envoie tel quel puis affiche les compteurs', async () => {
    let received: unknown
    server.use(
      http.post(`${API}/api/import`, async ({ request }) => {
        received = await request.json()
        return HttpResponse.json({ members: 1, accounts: 2, holdings: 3, goals: 0 })
      }),
    )
    const { user: actor } = await openData()
    await actor.upload(importInput(), jsonFile('sauvegarde.json', JSON.stringify(bundle)))

    const dialog = await screen.findByRole('dialog', { name: 'Importer ce fichier ?' })
    expect(pageText(dialog)).toContain('sauvegarde.json contient 1 membre(s), 2 compte(s) et 0 objectif(s).')
    await actor.click(within(dialog).getByRole('button', { name: 'Importer' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual(bundle)
    expect(screen.getByText(/Import terminé/)).toHaveTextContent(
      'Import terminé : 1 membre(s), 2 compte(s), 3 position(s) et 0 objectif(s) traités.',
    )
  })

  it('refuse un fichier qui n’est pas un export FineMe sans appeler l’API', async () => {
    const calls = spyOn('post', '/api/import')
    const { user: actor } = await openData()

    await actor.upload(importInput(), jsonFile('autre.json', JSON.stringify({ version: 2, members: [] })))
    expect(await screen.findByRole('alert')).toHaveTextContent('Ce fichier n’est pas un export FineMe')

    await actor.upload(importInput(), jsonFile('casse.json', '{ pas du json'))
    expect(await screen.findByText('Lecture impossible : le fichier n’est pas un JSON valide.')).toBeInTheDocument()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(calls.count).toBe(0)
  })

  it('garde la fenêtre ouverte avec le message de l’API quand l’import échoue', async () => {
    mockApiError('post', '/api/import', 422, 'Le compte « PEA Camille » référence un membre inconnu.')
    const { user: actor } = await openData()
    await actor.upload(importInput(), jsonFile('sauvegarde.json', JSON.stringify(bundle)))
    const dialog = await screen.findByRole('dialog', { name: 'Importer ce fichier ?' })
    await actor.click(within(dialog).getByRole('button', { name: 'Importer' }))
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'Le compte « PEA Camille » référence un membre inconnu.',
    )
  })
})

describe('connexions récentes', () => {
  const events: LoginEvent[] = [
    {
      at: '2026-03-02T08:15:00Z',
      action: 'login',
      success: true,
      ip: '198.51.100.4',
      user_agent: 'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0',
    },
    { at: '2026-03-01T22:40:00Z', action: 'reset', success: false, ip: '203.0.113.9', user_agent: null },
  ]

  async function openWith(setup: () => void) {
    mockShell()
    setup()
    renderApp('/parametres')
    await screen.findByRole('heading', { name: 'Connexions récentes' })
  }

  it('liste les tentatives avec leur issue, l’appareil et l’adresse IP', async () => {
    await openWith(() => mockJson('get', '/api/auth/logins', events))
    const list = await screen.findByRole('list', { name: 'Connexions récentes' })
    const items = within(list).getAllByRole('listitem')
    expect(items).toHaveLength(2)
    expect(items[0]).toHaveTextContent('Connexion réussie')
    expect(items[0]).toHaveTextContent('Firefox sur Linux · 198.51.100.4')
    expect(items[1]).toHaveTextContent('Réinitialisation refusée')
    expect(items[1]).toHaveTextContent('Appareil inconnu · 203.0.113.9')
    expect(screen.getByText(/2 dernières tentatives, dont 1 refusée\./)).toBeInTheDocument()
  })

  it('indique quand le journal est vide', async () => {
    await openWith(() => mockJson('get', '/api/auth/logins', []))
    expect(await screen.findByText('Aucune connexion enregistrée pour le moment.')).toBeInTheDocument()
  })

  it('affiche l’erreur de chargement', async () => {
    await openWith(() => mockApiError('get', '/api/auth/logins', 500, 'Journal indisponible.'))
    expect(await screen.findByRole('alert')).toHaveTextContent('Journal indisponible.')
  })
})
