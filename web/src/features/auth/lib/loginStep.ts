import type { LoginResult, TotpChallenge } from '@/shared/api/types'

/** Vrai quand la connexion attend encore le code du second facteur. */
export function isTotpChallenge(result: LoginResult): result is TotpChallenge {
  return 'challenge' in result && result.totp_required === true
}

/** Code saisi prêt à envoyer : espaces retirés (« 123 456 » → « 123456 »). */
export function cleanCode(input: string): string {
  return input.replace(/\s+/g, '')
}
