import { describe, expect, it } from 'vitest'
import { readAll, readField } from './fields'

describe('readField', () => {
  it('lit un nombre au format français', () => {
    expect(readField('1 234,5')).toBe(1234.5)
  })
  it('exige un champ obligatoire', () => {
    expect(readField('')).toBeNull()
    expect(readField('  ')).toBeNull()
  })
  it('vaut 0 pour un champ facultatif vide', () => {
    expect(readField('', { optional: true })).toBe(0)
  })
  it('refuse le négatif sauf indication contraire', () => {
    expect(readField('-2')).toBeNull()
    expect(readField('-2', { signed: true })).toBe(-2)
  })
  it('refuse une valeur au-dessus du maximum ou illisible', () => {
    expect(readField('120', { max: 100 })).toBeNull()
    expect(readField('abc')).toBeNull()
  })
})

describe('readAll', () => {
  it('renvoie les valeurs quand toutes sont lues', () => {
    expect(readAll({ a: 1, b: 0 })).toEqual({ a: 1, b: 0 })
  })
  it('renvoie null dès qu’une valeur manque', () => {
    expect(readAll({ a: 1, b: null })).toBeNull()
  })
})
