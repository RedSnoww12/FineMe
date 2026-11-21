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

/** Libellé de l'action et de son issue : « Connexion réussie », « Réinitialisation refusée »… */
export function describeAttempt(event: Pick<LoginEvent, 'action' | 'success'>): string {
  const action =
    event.action === 'reset' ? 'Réinitialisation' : event.action === 'totp' ? 'Vérification en deux étapes' : 'Connexion'
  return `${action} ${event.success ? 'réussie' : 'refusée'}`
}

/** Nombre d'échecs parmi les tentatives listées. */
export function failureCount(events: LoginEvent[]): number {
  return events.filter((event) => !event.success).length
}
