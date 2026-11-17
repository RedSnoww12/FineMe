import { describe, expect, it } from 'vitest'
import { MIN_PASSWORD_LENGTH, newPasswordErrors } from './password'

describe('newPasswordErrors', () => {
  it('accepte un mot de passe assez long et bien confirmé', () => {
    expect(newPasswordErrors('tournesol-42', 'tournesol-42')).toEqual({})
  })

  it('accepte exactement la longueur minimale', () => {
    const exact = 'a'.repeat(MIN_PASSWORD_LENGTH)
    expect(newPasswordErrors(exact, exact)).toEqual({})
  })

  it('refuse un mot de passe trop court', () => {
    const short = 'a'.repeat(MIN_PASSWORD_LENGTH - 1)
    expect(newPasswordErrors(short, short)).toEqual({
      password: 'Le mot de passe doit contenir au moins 10 caractères.',
    })
  })

  it('compte les caractères comme l’API, pas les unités UTF-16', () => {
    // 9 émojis = 18 unités UTF-16 mais 9 caractères : refusé par l'API, donc refusé ici.
    const emojis = '🌻'.repeat(9)
    expect(newPasswordErrors(emojis, emojis).password).toBeDefined()
  })

  it('signale une confirmation différente', () => {
    expect(newPasswordErrors('tournesol-42', 'tournesol-43')).toEqual({
      confirm: 'Les deux mots de passe ne correspondent pas.',
    })
  })

  it('cumule les deux erreurs', () => {
    expect(Object.keys(newPasswordErrors('court', ''))).toEqual(['password', 'confirm'])
  })
})
