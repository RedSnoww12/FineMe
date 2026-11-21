import { describe, expect, it } from 'vitest'
import { describeAttempt, describeUserAgent, failureCount } from './loginHistory'

describe('describeUserAgent', () => {
  it('résume navigateur et système', () => {
    expect(
      describeUserAgent('Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0'),
    ).toBe('Firefox sur Linux')
    expect(
      describeUserAgent(
        'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36 Edg/140.0',
      ),
    ).toBe('Edge sur Windows')
    expect(
      describeUserAgent(
        'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1',
      ),
    ).toBe('Safari sur iOS')
  })

  it('garde le texte brut quand rien n’est reconnu', () => {
    expect(describeUserAgent('curl/8.5.0')).toBe('curl')
    expect(describeUserAgent('robot-maison')).toBe('robot-maison')
    expect(describeUserAgent(null)).toBe('Appareil inconnu')
  })
})

describe('describeAttempt', () => {
  it('nomme l’action et son issue', () => {
    expect(describeAttempt({ action: 'login', success: true })).toBe('Connexion réussie')
    expect(describeAttempt({ action: 'reset', success: false })).toBe('Réinitialisation refusée')
  })
})

describe('failureCount', () => {
  it('compte les échecs', () => {
    const base = { at: '2026-01-01T10:00:00Z', action: 'login' as const, ip: null, user_agent: null }
    expect(failureCount([])).toBe(0)
    expect(failureCount([{ ...base, success: false }, { ...base, success: true }, { ...base, success: false }])).toBe(2)
  })
})
