import type { Alert, AlertKind } from '@/shared/api/types'

/** Au-delà, le badge affiche « 9+ » (le nombre exact reste lu par les lecteurs d'écran). */
const BADGE_MAX = 9

/** Alertes dont le texte cite des montants : masqué en mode discret. */
const KINDS_WITH_AMOUNTS: AlertKind[] = ['savings_ceiling', 'net_worth_change']

export function unreadCount(alerts: Alert[]): number {
  return alerts.filter((alert) => !alert.read).length
}

/** « Aucune notification non lue », « 1 notification non lue », « 3 notifications non lues ». */
export function unreadLabel(count: number): string {
  if (count === 0) return 'Aucune notification non lue'
  return count === 1 ? '1 notification non lue' : `${count} notifications non lues`
}

export function badgeText(count: number): string {
  return count > BADGE_MAX ? `${BADGE_MAX}+` : String(count)
}

/** Texte à afficher : les montants ne doivent pas apparaître en mode discret. */
export function alertMessage(alert: Alert, amountsHidden: boolean): string {
  if (amountsHidden && KINDS_WITH_AMOUNTS.includes(alert.kind)) return 'Montants masqués (mode discret).'
  return alert.message
}

/** Non lues d'abord, puis les plus récentes ; les alertes résolues après celles qui sont en cours. */
export function sortForDisplay(alerts: Alert[]): Alert[] {
  return [...alerts].sort(
    (a, b) =>
      Number(a.read) - Number(b.read) ||
      Number(b.active) - Number(a.active) ||
      b.created_at.localeCompare(a.created_at),
  )
}
