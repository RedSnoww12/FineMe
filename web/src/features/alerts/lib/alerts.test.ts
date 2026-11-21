import { describe, expect, it } from 'vitest'
import type { Alert } from '@/shared/api/types'
import { alertMessage, badgeText, sortForDisplay, unreadCount, unreadLabel } from './alerts'

function alert(overrides: Partial<Alert> = {}): Alert {
  return {
    id: 'a-1',
    kind: 'stale_prices',
    severity: 'info',
    title: 'Titre',
    message: 'Message',
    created_at: '2026-03-01T07:00:00Z',
    read: false,
    archived: false,
    active: true,
    ...overrides,
  }
}

describe('notifications', () => {
  it('compte les non lues et accorde le libellé', () => {
    const alerts = [alert(), alert({ id: 'a-2', read: true }), alert({ id: 'a-3' })]
    expect(unreadCount(alerts)).toBe(2)
    expect(unreadCount([])).toBe(0)
    expect(unreadLabel(0)).toBe('Aucune notification non lue')
    expect(unreadLabel(1)).toBe('1 notification non lue')
    expect(unreadLabel(2)).toBe('2 notifications non lues')
  })

  it('plafonne le badge à 9+', () => {
    expect(badgeText(9)).toBe('9')
    expect(badgeText(10)).toBe('9+')
  })

  it('masque les montants des alertes qui en citent en mode discret', () => {
    const ceiling = alert({ kind: 'savings_ceiling', message: 'Livret à 95 % (21 000 €)' })
    expect(alertMessage(ceiling, false)).toBe('Livret à 95 % (21 000 €)')
    expect(alertMessage(ceiling, true)).not.toContain('21 000')
    expect(alertMessage(alert({ message: 'Cours à vérifier' }), true)).toBe('Cours à vérifier')
  })

  it('trie : non lues, puis en cours, puis les plus récentes', () => {
    const sorted = sortForDisplay([
      alert({ id: 'lue', read: true, created_at: '2026-03-05T00:00:00Z' }),
      alert({ id: 'ancienne', created_at: '2026-03-01T00:00:00Z' }),
      alert({ id: 'résolue', active: false, created_at: '2026-03-06T00:00:00Z' }),
      alert({ id: 'récente', created_at: '2026-03-04T00:00:00Z' }),
    ])
    expect(sorted.map((a) => a.id)).toEqual(['récente', 'ancienne', 'résolue', 'lue'])
  })
})
