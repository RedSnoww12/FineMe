import { describe, expect, it } from 'vitest'
import { approvalInput, expiryTime, isExpectedRedirect, isRequestId } from './consent'

describe('isRequestId', () => {
  it('n’accepte qu’un identifiant de demande bien formé', () => {
    expect(isRequestId(`fm_oar_${'a'.repeat(43)}`)).toBe(true)
    for (const value of ['', '.', '..', 'fm_oar_court', `fm_oac_${'a'.repeat(43)}`, `fm_oar_${'a'.repeat(42)}/`]) {
      expect(isRequestId(value)).toBe(false)
    }
  })
})

describe('expiryTime', () => {
  it('donne l’heure de fin de validité', () => {
    const readAt = new Date(2026, 0, 2, 14, 0, 0).getTime()
    expect(expiryTime(readAt, 600)).toBe('14:10')
  })
})

describe('approvalInput', () => {
  it('n’envoie le code que s’il est saisi, sans espaces', () => {
    expect(approvalInput('secret', '')).toEqual({ password: 'secret' })
    expect(approvalInput('secret', '  ')).toEqual({ password: 'secret' })
    expect(approvalInput('secret', '123 456')).toEqual({ password: 'secret', code: '123456' })
  })
})

describe('isExpectedRedirect', () => {
  it('n’accepte que l’hôte montré, en https ou en local', () => {
    expect(isExpectedRedirect('https://claude.ai/cb?code=x', 'claude.ai')).toBe(true)
    expect(isExpectedRedirect('https://claude.ai/cb', 'claude.ai:443')).toBe(true)
    expect(isExpectedRedirect('http://localhost:3118/cb', 'localhost:3118')).toBe(true)
    expect(isExpectedRedirect('http://[::1]:3118/cb', '[::1]:3118')).toBe(true)
    expect(isExpectedRedirect('http://claude.ai/cb', 'claude.ai')).toBe(false)
    expect(isExpectedRedirect('https://claude.ai.example.test/cb', 'claude.ai')).toBe(false)
    expect(isExpectedRedirect('https://claude.ai@example.test/cb', 'claude.ai')).toBe(false)
    expect(isExpectedRedirect('http://localhost:9999/cb', 'localhost:3118')).toBe(false)
    expect(isExpectedRedirect('javascript:alert(1)', 'claude.ai')).toBe(false)
    expect(isExpectedRedirect('/relatif', 'claude.ai')).toBe(false)
  })
})
