import type { Overview } from '@/shared/api/types'
import { members } from './fixtures'
import { mockJson } from './server'

/** Requêtes émises par le shell sur tous les écrans connectés. */
export function mockShell() {
  mockJson('get', '/api/members', members)
}

/** Requêtes de la vue d'ensemble, sans historique. */
export function mockOverviewScreen(overview: Overview) {
  mockJson('get', '/api/overview', overview)
  mockJson('get', '/api/history', { points: [] })
}
