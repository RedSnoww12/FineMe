import { describe, expect, it } from 'vitest'
import { REFRESH_AFTER_MS, shouldRefresh, tokenIssuedAt } from './token'

/** Jeton fictif non signé : seul le contenu compte ici. */
function fakeJwt(claims: object): string {
  const encode = (value: object) =>
    btoa(JSON.stringify(value)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  return `${encode({ alg: 'HS256', typ: 'JWT' })}.${encode(claims)}.signature`
}

const ISSUED = Date.UTC(2026, 0, 1, 12) // ms

describe('tokenIssuedAt', () => {
  it('lit la date d’émission d’un JWT', () => {
    expect(tokenIssuedAt(fakeJwt({ sub: 'u', iat: ISSUED / 1000 }))).toBe(ISSUED)
  })

  it('renvoie null pour un jeton illisible ou sans iat', () => {
    expect(tokenIssuedAt('jeton-de-test')).toBeNull()
    expect(tokenIssuedAt('a.%%%.c')).toBeNull()
    expect(tokenIssuedAt(fakeJwt({ sub: 'u' }))).toBeNull()
  })
})

describe('shouldRefresh', () => {
  const token = fakeJwt({ iat: ISSUED / 1000 })

  it('renouvelle un jeton émis il y a plus d’un jour seulement', () => {
    expect(shouldRefresh(token, ISSUED + REFRESH_AFTER_MS - 1)).toBe(false)
    expect(shouldRefresh(token, ISSUED + REFRESH_AFTER_MS)).toBe(true)
  })

  it('ne tente rien sur un jeton illisible', () => {
    expect(shouldRefresh('jeton-de-test', ISSUED + 10 * REFRESH_AFTER_MS)).toBe(false)
  })
})
