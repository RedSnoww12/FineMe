/**
 * Quitte l'application pour l'adresse de retour du client OAuth, en remplaçant l'écran de consentement dans
 * l'historique. Isolé pour pouvoir être observé dans les tests.
 */
export function leaveTo(url: string): void {
  window.location.replace(url)
}
