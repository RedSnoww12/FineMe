import type { LoginEvent } from '@/shared/api/types'

const BROWSERS: [RegExp, string][] = [
  [/Edg\//, 'Edge'],
  [/OPR\/|Opera/, 'Opera'],
  [/Firefox\//, 'Firefox'],
  [/Chrome\/|CriOS\//, 'Chrome'],
  [/Safari\//, 'Safari'],
  [/curl\//i, 'curl'],
]

const SYSTEMS: [RegExp, string][] = [
  [/iPhone|iPad|iPod/, 'iOS'],
  [/Android/, 'Android'],
  [/Windows/, 'Windows'],
  [/Mac OS X|Macintosh/, 'macOS'],
  [/Linux/, 'Linux'],
]

function firstMatch(value: string, table: [RegExp, string][]): string | null {
  return table.find(([pattern]) => pattern.test(value))?.[1] ?? null
}

/** Résumé lisible d'un user-agent : « Firefox sur Linux », sinon le texte brut, ou « Appareil inconnu ». */
export function describeUserAgent(userAgent: string | null): string {
  if (!userAgent) return 'Appareil inconnu'
  const browser = firstMatch(userAgent, BROWSERS)
  const system = firstMatch(userAgent, SYSTEMS)
  if (browser && system) return `${browser} sur ${system}`
  return browser ?? system ?? userAgent
}

const ACTIONS: Record<LoginEvent['action'], string> = {
  login: 'Connexion',
  totp: 'Vérification en deux étapes',
  reset: 'Réinitialisation',
  oauth: 'Autorisation OAuth',
}

/** Libellé de l'action et de son issue : « Connexion réussie », « Réinitialisation refusée »… */
export function describeAttempt(event: Pick<LoginEvent, 'action' | 'success'>): string {
  // Une action ajoutée par une API plus récente reste lisible.
  return `${ACTIONS[event.action] ?? ACTIONS.login} ${event.success ? 'réussie' : 'refusée'}`
}

/** Nombre d'échecs parmi les tentatives listées. */
export function failureCount(events: LoginEvent[]): number {
  return events.filter((event) => !event.success).length
}
