import { describe, expect, it } from 'vitest'
import { cleanCode, isTotpChallenge } from './loginStep'

describe('étape de connexion', () => {
  it('distingue une session d’un défi de second facteur', () => {
    const user = { id: 'u-1', email: 'camille@exemple.test', name: 'Camille' }
    expect(isTotpChallenge({ token: 'jeton', user })).toBe(false)
    expect(isTotpChallenge({ totp_required: true, challenge: 'defi', expires_in: 300 })).toBe(true)
  })

  it('retire les espaces du code saisi', () => {
    expect(cleanCode(' 123 456 ')).toBe('123456')
    expect(cleanCode('abcde-fgh89')).toBe('abcde-fgh89')
  })
})
