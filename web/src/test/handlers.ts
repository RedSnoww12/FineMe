import type { Overview } from '@/shared/api/types'
import { members } from './fixtures'
import { mockJson } from './server'

/** Requêtes émises par le shell sur tous les écrans connectés. */
export function mockShell() {
  mockJson('get', '/api/members', members)
  mockJson('get', '/api/alerts', [])
  // Panneau du second facteur (Paramètres) : désactivé par défaut.
  mockJson('get', '/api/auth/totp', { enabled: false, recovery_codes_remaining: 0 })
  // Panneau Accès IA (Paramètres) : aucun jeton, aucun appel.
  mockJson('get', '/api/access-tokens', [])
  mockJson('get', '/api/access-tokens/calls', [])
}

/** Requêtes de la vue d'ensemble, sans historique. */
export function mockOverviewScreen(overview: Overview) {
  mockJson('get', '/api/overview', overview)
  mockJson('get', '/api/history', { points: [] })
}
