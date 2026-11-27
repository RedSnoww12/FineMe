// Écran de consentement OAuth : présentation de la demande, saisie de la confirmation et retour vers le client.

/** Forme d'un identifiant de demande (256 bits en base64url) : rien d'autre n'est envoyé à l'API. */
const REQUEST_ID = /^fm_oar_[A-Za-z0-9_-]{43}$/
const LOOPBACK = ['localhost', '127.0.0.1', '[::1]']

export function isRequestId(value: string): boolean {
  return REQUEST_ID.test(value)
}

/** Heure à laquelle la demande lue à `readAt` (ms) cesse d'être valable : « 14:05 ». */
export function expiryTime(readAt: number, secondsLeft: number): string {
  return new Date(readAt + secondsLeft * 1000).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' })
}

/** Corps de l'approbation : le code, facultatif, est envoyé sans espaces et seulement s'il est saisi. */
export function approvalInput(password: string, code: string): { password: string; code?: string } {
  const cleaned = code.replace(/\s+/g, '')
  return { password, ...(cleaned ? { code: cleaned } : {}) }
}

/**
 * Vrai si `target` mène bien à l'hôte montré à l'écran, en `https` (ou `http` sur la machine locale). L'API le
 * garantit déjà ; le vérifier ici évite de partir ailleurs que ce qui a été consenti.
 */
export function isExpectedRedirect(target: string, redirectHost: string): boolean {
  try {
    const url = new URL(target)
    const secure = url.protocol === 'https:' || (url.protocol === 'http:' && LOOPBACK.includes(url.hostname))
    // Même schéma des deux côtés : un port par défaut explicite ne fait pas de différence.
    return secure && url.host === new URL(`${url.protocol}//${redirectHost}`).host
  } catch {
    return false
  }
}
