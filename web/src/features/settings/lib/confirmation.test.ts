import { describe, expect, it } from 'vitest'
import { confirmationErrors, confirmationInput } from './confirmation'

describe('confirmationErrors', () => {
  it('exige le mot de passe, et le code seulement avec le second facteur', () => {
    expect(Object.keys(confirmationErrors({ password: '', code: '' }, false))).toEqual(['password'])
    expect(Object.keys(confirmationErrors({ password: 'secret', code: ' ' }, true))).toEqual(['code'])
    expect(confirmationErrors({ password: 'secret', code: '' }, false)).toEqual({})
    expect(confirmationErrors({ password: 'secret', code: '123456' }, true)).toEqual({})
  })
})

describe('confirmationInput', () => {
  it('retire les espaces du code et l’omet s’il est vide', () => {
    expect(confirmationInput({ password: 'secret', code: '123 456' })).toEqual({ password: 'secret', code: '123456' })
    expect(confirmationInput({ password: 'secret', code: '  ' })).toEqual({ password: 'secret' })
  })
})
