import { waitFor, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { HistoryReview } from '@/shared/api/types'
import { date, todayIso } from '@/shared/lib/format'
import { buildHistoryReviewPoint, camille } from '@/test/fixtures'
import { mockShell } from '@/test/handlers'
import { pageText, renderApp, screen } from '@/test/render'
import { API, http, HttpResponse, mockApiError, mockJson, server } from '@/test/server'

const TODAY = todayIso()

/** Trois jours fictifs dont un pic aberrant le 2 mars, puis le point du jour. */
const review: HistoryReview = {
  threshold: 0.2,
  points: [
    buildHistoryReviewPoint('2026-03-01', { net_worth: 100_000 }),
    buildHistoryReviewPoint('2026-03-02', { net_worth: 10_000, change: -0.9, suspicious: true }),
    buildHistoryReviewPoint('2026-03-03', { net_worth: 101_000, change: 9.1 }),
    buildHistoryReviewPoint(TODAY, { net_worth: 101_500, change: 0.005 }),
  ],
}

/** Sert `review`, puis `next` après la première écriture ; consigne les `member_id` reçus. */
function mockReview(next: HistoryReview = review) {
  const state = { current: review, reads: 0, members: [] as (string | null)[] }
  server.use(
    http.get(`${API}/api/history/points`, ({ request }) => {
      state.reads += 1
      state.members.push(new URL(request.url).searchParams.get('member_id'))
      return HttpResponse.json(state.current)
    }),
  )
  return {
    state,
    afterWrite: () => {
      state.current = next
    },
  }
}

async function openScreen() {
  mockShell()
  const view = renderApp('/parametres/historique')
  await screen.findByRole('heading', { name: 'Historique des graphiques' })
  return view
}

function rows() {
  return screen.getAllByRole('row').slice(1)
}

describe('écran de nettoyage de l’historique', () => {
  it('liste les points du plus récent au plus ancien et signale le pic', async () => {
    mockReview()
    const { user } = await openScreen()
    await screen.findByRole('table')

    const dates = rows().map((row) => within(row).getAllByRole('cell')[0].textContent)
    expect(dates).toEqual([date(TODAY), '03/03/2026', '02/03/2026', '01/03/2026'])
    const spike = rows()[2]
    expect(within(spike).getByText('Suspect')).toBeInTheDocument()
    expect(pageText(spike)).toContain('10 000 €')
    expect(pageText()).toContain('plus de 20 %')
    // Le point du jour ne se corrige pas.
    expect(screen.getByRole('button', { name: `Corriger le point du ${date(TODAY)}` })).toBeDisabled()
    expect(within(rows()[0]).getByText('Aujourd’hui')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Suspects (1)' }))
    expect(rows()).toHaveLength(1)
    expect(rows()[0]).toHaveTextContent('02/03/2026')
  })

  it('demande l’historique de la vue affichée', async () => {
    localStorage.setItem('fineme.view', camille.id)
    const { state } = mockReview()
    await openScreen()
    await screen.findByRole('table')
    expect(state.members).toContain(camille.id)
    expect(pageText()).toContain('Vue « Camille »')
  })

  it('corrige un point puis recharge la liste', async () => {
    const corrected = buildHistoryReviewPoint('2026-03-02', { net_worth: 100_500, gross_assets: 101_000, liabilities: 500, corrected: true })
    const { state, afterWrite } = mockReview({ ...review, points: review.points.map((p) => (p.date === '2026-03-02' ? corrected : p)) })
    let received: unknown
    server.use(
      http.put(`${API}/api/history/points/2026-03-02`, async ({ request }) => {
        received = await request.json()
        afterWrite()
        return HttpResponse.json(corrected)
      }),
    )
    const { user } = await openScreen()
    await user.click(await screen.findByRole('button', { name: 'Corriger le point du 02/03/2026' }))

    const dialog = screen.getByRole('dialog', { name: 'Corriger le point du 02/03/2026' })
    const gross = within(dialog).getByLabelText('Actifs bruts')
    await user.clear(gross)
    await user.click(within(dialog).getByRole('button', { name: 'Enregistrer la correction' }))
    expect(within(dialog).getByText('Indiquez les actifs bruts.')).toBeInTheDocument()

    await user.type(gross, '101 000')
    await user.clear(within(dialog).getByLabelText('Dettes'))
    await user.type(within(dialog).getByLabelText('Dettes'), '500')
    expect(pageText(dialog)).toContain('100 500 €')
    await user.click(within(dialog).getByRole('button', { name: 'Enregistrer la correction' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(received).toEqual({ gross_assets: 101_000, liabilities: 500 })
    await waitFor(() => expect(within(rows()[2]).getByText('Corrigé')).toBeInTheDocument())
    expect(state.reads).toBeGreaterThan(1)
  })

  it('supprime un point après confirmation et rafraîchit les graphiques', async () => {
    const { afterWrite } = mockReview({ ...review, points: review.points.filter((p) => p.date !== '2026-03-02') })
    let query = ''
    server.use(
      http.delete(`${API}/api/history/points`, ({ request }) => {
        query = new URL(request.url).search
        afterWrite()
        return HttpResponse.json({ deleted_days: 1 })
      }),
    )
    const { user } = await openScreen()
    await user.click(await screen.findByRole('button', { name: 'Supprimer le point du 02/03/2026' }))
    const dialog = screen.getByRole('dialog', { name: 'Supprimer le point du 02/03/2026 ?' })
    expect(dialog).toHaveTextContent('foyer et de chaque membre')
    await user.click(within(dialog).getByRole('button', { name: 'Supprimer le point' }))

    expect(await screen.findByText('1 jour supprimé de l’historique.')).toBeInTheDocument()
    expect(query).toBe('?from=2026-03-02&to=2026-03-02')
    await waitFor(() => expect(rows()).toHaveLength(3))
    expect(screen.queryByText('Suspect')).not.toBeInTheDocument()
  })

  it('supprime une période validée et confirmée', async () => {
    mockReview()
    let query = ''
    server.use(
      http.delete(`${API}/api/history/points`, ({ request }) => {
        query = new URL(request.url).search
        return HttpResponse.json({ deleted_days: 2 })
      }),
    )
    const { user } = await openScreen()
    await screen.findByRole('table')
    await user.click(screen.getByRole('button', { name: 'Supprimer la période' }))
    expect(screen.getByText('Indiquez la date de début.')).toBeInTheDocument()

    await user.type(screen.getByLabelText('Du'), '2026-03-03')
    await user.type(screen.getByLabelText('Au (inclus)'), '2026-03-01')
    await user.click(screen.getByRole('button', { name: 'Supprimer la période' }))
    expect(screen.getByText('La date de fin doit suivre la date de début.')).toBeInTheDocument()

    await user.clear(screen.getByLabelText('Du'))
    await user.type(screen.getByLabelText('Du'), '2026-03-02')
    await user.clear(screen.getByLabelText('Au (inclus)'))
    await user.type(screen.getByLabelText('Au (inclus)'), '2026-03-03')
    await user.click(screen.getByRole('button', { name: 'Supprimer la période' }))
    const dialog = screen.getByRole('dialog', { name: 'Supprimer cette période ?' })
    expect(dialog).toHaveTextContent('2 point(s)')
    expect(query).toBe('')
    await user.click(within(dialog).getByRole('button', { name: 'Supprimer la période' }))

    expect(await screen.findByText('2 jours supprimés de l’historique.')).toBeInTheDocument()
    expect(query).toBe('?from=2026-03-02&to=2026-03-03')
  })

  it('recalcule le point du jour', async () => {
    mockReview()
    mockJson('post', '/api/history/recompute', { snapshot_date: '2026-03-04' })
    const { user } = await openScreen()
    await user.click(screen.getByRole('button', { name: 'Recalculer aujourd’hui' }))
    expect(await screen.findByText('Point du 04/03/2026 recalculé à partir des données actuelles.')).toBeInTheDocument()
  })

  it('n’efface tout l’historique qu’après saisie du mot de confirmation', async () => {
    const { afterWrite } = mockReview({ threshold: 0.2, points: [] })
    let received: unknown
    server.use(
      http.post(`${API}/api/history/reset`, async ({ request }) => {
        received = await request.json()
        afterWrite()
        return HttpResponse.json({ deleted_days: 4 })
      }),
    )
    const { user } = await openScreen()
    await screen.findByRole('table')
    await user.click(screen.getByRole('button', { name: 'Effacer l’historique' }))
    const dialog = screen.getByRole('dialog', { name: 'Effacer tout l’historique ?' })
    const confirm = within(dialog).getByRole('button', { name: 'Effacer définitivement' })
    expect(confirm).toBeDisabled()
    await user.type(within(dialog).getByLabelText('Saisissez EFFACER pour confirmer'), 'effacer')
    expect(confirm).toBeDisabled()
    await user.clear(within(dialog).getByLabelText('Saisissez EFFACER pour confirmer'))
    await user.type(within(dialog).getByLabelText('Saisissez EFFACER pour confirmer'), 'EFFACER')
    expect(confirm).toBeEnabled()
    await user.click(confirm)

    await waitFor(() => expect(received).toEqual({ confirmation: 'EFFACER' }))
    expect(await screen.findByRole('heading', { name: 'Aucun point d’historique' })).toBeInTheDocument()
  })

  it('affiche l’erreur de chargement', async () => {
    mockApiError('get', '/api/history/points', 500, 'Lecture de l’historique impossible.')
    await openScreen()
    expect(await screen.findByRole('alert')).toHaveTextContent('Lecture de l’historique impossible.')
  })

  it('est accessible depuis les paramètres', async () => {
    mockShell()
    mockReview()
    mockJson('get', '/api/auth/logins', [])
    mockJson('get', '/api/alerts/rules', [])
    const { user } = renderApp('/parametres')
    await user.click(await screen.findByRole('link', { name: 'Nettoyer l’historique' }))
    expect(await screen.findByRole('heading', { name: 'Historique des graphiques' })).toBeInTheDocument()
  })
})
