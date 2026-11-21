import { waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Account, Transaction } from '@/shared/api/types'
import { MASK } from '@/shared/lib/format'
import {
  buildAccount,
  buildAccountDetail,
  buildGoal,
  buildHistoryPoint,
  buildHolding,
  buildOverview,
  camille,
  sacha,
} from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

const checking = buildAccount()
const pea = buildAccount({
  id: 'a-pea',
  name: 'PEA Camille',
  kind: 'investment',
  envelope: 'pea',
  owners: [{ member_id: camille.id, name: camille.name, share: 1 }],
  value: 33_000,
  owned_value: 33_000,
})

/** Sert les lectures du rapport et note le `member_id` reçu par chaque route. */
function serveReport(accounts: Account[] = [checking, pea]) {
  const seen: Record<string, string | null> = {}
  const record = (name: string, request: Request) => {
    seen[name] = new URL(request.url).searchParams.get('member_id')
  }
  server.use(
    http.get(`${API}/api/overview`, ({ request }) => {
      record('overview', request)
      return HttpResponse.json(buildOverview())
    }),
    http.get(`${API}/api/accounts`, ({ request }) => {
      record('accounts', request)
      return HttpResponse.json(accounts)
    }),
    http.get(`${API}/api/goals`, ({ request }) => {
      record('goals', request)
      return HttpResponse.json([buildGoal()])
    }),
    http.get(`${API}/api/history`, ({ request }) => {
      record('history', request)
      return HttpResponse.json({
        points: [
          buildHistoryPoint('2025-12-31', null, { gross_assets: 160_000, liabilities: 60_000, net_worth: 100_000 }),
          buildHistoryPoint('2026-01-31', null, { gross_assets: 180_000, liabilities: 60_000, net_worth: 120_000 }),
        ],
      })
    }),
  )
  return seen
}

describe('rapport de patrimoine', () => {
  it('présente synthèse, répartition, évolution, objectifs et comptes du foyer', async () => {
    mockShell()
    const seen = serveReport()
    renderApp('/parametres/rapport?membre=foyer')

    expect(await screen.findByRole('heading', { name: 'Rapport de patrimoine', level: 1 })).toBeInTheDocument()
    await screen.findByRole('heading', { name: 'Synthèse' })
    for (const title of ['Par membre du foyer', 'Répartition des actifs', 'Évolution sur douze mois', 'Objectifs', 'Liste des comptes']) {
      expect(screen.getByRole('heading', { name: title })).toBeInTheDocument()
    }
    const text = pageText()
    expect(text).toContain('Ensemble du foyer')
    expect(text).toContain('120 000 €')
    expect(text).toContain('+20 000 € (+20,0 %)')
    expect(text).toContain('Apport appartement')
    expect(text).toContain('PEA Camille · PEA')
    expect(seen).toEqual({ overview: null, accounts: null, goals: null, history: null })
  })

  it('demande la part du membre choisi et l’imprime en un clic', async () => {
    mockShell()
    const seen = serveReport([pea])
    const print = vi.spyOn(window, 'print').mockImplementation(() => {})
    const { user } = renderApp(`/parametres/rapport?membre=${camille.id}`)

    await screen.findByRole('heading', { name: 'Synthèse' })
    expect(pageText()).toContain('Part détenue par Camille')
    expect(screen.queryByRole('heading', { name: 'Par membre du foyer' })).not.toBeInTheDocument()
    expect(seen.accounts).toBe(camille.id)
    expect(seen.goals).toBe(camille.id)

    await user.selectOptions(screen.getByLabelText('Périmètre'), sacha.id)
    await waitFor(() => expect(seen.overview).toBe(sacha.id))

    await user.click(screen.getByRole('button', { name: 'Imprimer ou enregistrer en PDF' }))
    expect(print).toHaveBeenCalledOnce()
  })

  it('masque les montants en mode discret et le signale', async () => {
    localStorage.setItem('fineme.discreet', 'on')
    mockShell()
    serveReport()
    renderApp('/parametres/rapport')

    await screen.findByRole('heading', { name: 'Synthèse' })
    expect(screen.getByText(/Mode discret actif : les montants sont masqués/)).toBeInTheDocument()
    expect(pageText()).toContain(`${MASK} €`)
    expect(pageText()).not.toContain('120 000 €')
  })

  it('affiche l’erreur de l’API', async () => {
    mockShell()
    serveReport()
    mockApiError('get', '/api/overview', 500, 'Synthèse indisponible.')
    renderApp('/parametres/rapport')
    expect(await screen.findByText('Synthèse indisponible.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Imprimer ou enregistrer en PDF' })).toBeDisabled()
  })

  it('reste lisible sans historique ni objectif', async () => {
    mockShell()
    serveReport()
    mockJson('get', '/api/goals', [])
    mockJson('get', '/api/history', { points: [] })
    renderApp('/parametres/rapport')
    expect(await screen.findByText('Aucun objectif défini.')).toBeInTheDocument()
    expect(screen.getByText(/Pas encore assez d’historique/)).toBeInTheDocument()
  })
})

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

function tx(id: string, label: string, amount: number): Transaction {
  return { id, account_id: checking.id, date: '2026-02-10', label, amount, category: null, manual_category: null, internal_transfer: null }
}

/** Comptes, détail et transactions (deux pages) servis à l'export ; note le `member_id` des comptes. */
function serveExport() {
  const seen: { member: string | null; offsets: string[] } = { member: null, offsets: [] }
  server.use(
    http.get(`${API}/api/accounts`, ({ request }) => {
      seen.member = new URL(request.url).searchParams.get('member_id')
      return HttpResponse.json([checking, pea])
    }),
    http.get(`${API}/api/accounts/:id`, ({ params }) =>
      HttpResponse.json(params.id === pea.id ? buildAccountDetail({ ...pea, holdings: [buildHolding({ account_id: pea.id })] }) : buildAccountDetail()),
    ),
    http.get(`${API}/api/accounts/:id/transactions`, ({ params, request }) => {
      if (params.id !== checking.id) return HttpResponse.json({ items: [], total: 0 })
      const offset = new URL(request.url).searchParams.get('offset') ?? '0'
      seen.offsets.push(offset)
      // L'API pagine : le second appel reprend là où le premier s'est arrêté.
      return HttpResponse.json(
        offset === '0'
          ? { items: [tx('t-1', 'Boulangerie; centre', -4.5)], total: 2 }
          : { items: [tx('t-2', '=SOMME(A1)', 1200)], total: 2 },
      )
    }),
  )
  return seen
}

async function openExport() {
  mockShell()
  mockJson('get', '/api/auth/logins', [])
  const view = renderApp('/parametres')
  await screen.findByRole('heading', { name: 'Rapport et exports' })
  return view
}

describe('exports CSV et Excel', () => {
  it('télécharge les transactions en CSV français, toutes pages lues', async () => {
    const seen = serveExport()
    const captured = captureDownload()
    const { user } = await openExport()
    await user.selectOptions(screen.getByLabelText('Fichier'), 'transactions')
    await user.click(screen.getByRole('button', { name: 'Exporter' }))

    await waitFor(() => expect(captured.fileName).toMatch(/^fineme-transactions-foyer-\d{4}-\d{2}-\d{2}\.csv$/))
    const csv = await (captured.blob as Blob).text()
    // Blob.text() retire le BOM au décodage : on relit les octets.
    const bytes = new Uint8Array(await (captured.blob as Blob).arrayBuffer())
    expect([...bytes.slice(0, 3)]).toEqual([0xef, 0xbb, 0xbf])
    const lines = csv.replace(/^﻿/, '').split('\r\n')
    expect(lines[0]).toBe('Compte;Date;Libellé;Montant;Devise;Catégorie;Part de la vue (%);Montant détenu')
    expect(lines[1]).toBe('Compte joint;10/02/2026;"Boulangerie; centre";-4,5;EUR;;100;-4,5')
    expect(lines[2]).toBe("Compte joint;10/02/2026;'=SOMME(A1);1200;EUR;;100;1200")
    expect(seen.offsets).toEqual(['0', '1'])
  })

  it('exporte un classeur Excel pour le membre choisi', async () => {
    const seen = serveExport()
    const captured = captureDownload()
    const { user } = await openExport()
    const panel = within(screen.getByRole('heading', { name: 'Rapport et exports' }).closest('section') as HTMLElement)
    await panel.findByRole('option', { name: sacha.name })
    await user.selectOptions(panel.getByLabelText('Périmètre'), sacha.id)
    await user.click(panel.getByRole('button', { name: 'Exporter' }))

    await waitFor(() => expect(captured.fileName).toMatch(/^fineme-patrimoine-sacha-\d{4}-\d{2}-\d{2}\.xlsx$/))
    expect(seen.member).toBe(sacha.id)
    const bytes = new Uint8Array(await (captured.blob as Blob).arrayBuffer())
    // Signature d'une archive ZIP (« PK\x03\x04 »).
    expect([...bytes.slice(0, 4)]).toEqual([0x50, 0x4b, 0x03, 0x04])
    expect(panel.getByRole('link', { name: 'Préparer le rapport PDF' })).toHaveAttribute(
      'href',
      `/parametres/rapport?membre=${sacha.id}`,
    )
  })

  it('affiche l’erreur de l’API sans rien télécharger', async () => {
    mockApiError('get', '/api/accounts', 500, 'Comptes indisponibles.')
    const captured = captureDownload()
    const { user } = await openExport()
    await user.click(screen.getByRole('button', { name: 'Exporter' }))
    expect(await screen.findByText('Comptes indisponibles.')).toBeInTheDocument()
    expect(captured.fileName).toBeUndefined()
  })
})
