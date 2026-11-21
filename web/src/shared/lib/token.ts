/** Âge à partir duquel le jeton est renouvelé (`POST /api/auth/refresh`) ; le jeton vit 7 jours (docs/API.md). */
export const REFRESH_AFTER_MS = 24 * 60 * 60 * 1000

function decodeBase64Url(segment: string): string {
  const base64 = segment.replace(/-/g, '+').replace(/_/g, '/')
  return atob(base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), '='))
}

/** Date d'émission (`iat`, en ms) d'un JWT, sans vérifier sa signature ; null si illisible. */
export function tokenIssuedAt(token: string): number | null {
  const payload = token.split('.')[1]
  if (!payload) return null
  try {
    const claims: unknown = JSON.parse(decodeBase64Url(payload))
    if (claims && typeof claims === 'object' && 'iat' in claims && typeof claims.iat === 'number') {
      return claims.iat * 1000
    }
    return null
  } catch {
    return null
  }
}

/** Vrai si le jeton a été émis il y a plus de `REFRESH_AFTER_MS` (renouvellement glissant). */
export function shouldRefresh(token: string, now: number): boolean {
  const issuedAt = tokenIssuedAt(token)
  return issuedAt !== null && now - issuedAt >= REFRESH_AFTER_MS
}
