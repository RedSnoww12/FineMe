import { describe, expect, it } from 'vitest'
import { seededRandom, standardNormal } from './random'

const draw = (next: () => number, count: number) => Array.from({ length: count }, () => next())

describe('seededRandom', () => {
  it('produit la même suite pour la même graine', () => {
    expect(draw(seededRandom(42), 20)).toEqual(draw(seededRandom(42), 20))
  })
  it('produit une suite différente pour une autre graine', () => {
    expect(draw(seededRandom(42), 20)).not.toEqual(draw(seededRandom(43), 20))
  })
  it('reste dans [0, 1) avec une moyenne proche de 0,5', () => {
    const values = draw(seededRandom(7), 20_000)
    expect(values.every((v) => v >= 0 && v < 1)).toBe(true)
    const mean = values.reduce((sum, v) => sum + v, 0) / values.length
    expect(mean).toBeCloseTo(0.5, 1)
  })
})

describe('standardNormal', () => {
  it('est reproductible', () => {
    expect(draw(standardNormal(seededRandom(1)), 11)).toEqual(draw(standardNormal(seededRandom(1)), 11))
  })
  it('suit une loi centrée réduite', () => {
    const values = draw(standardNormal(seededRandom(3)), 40_000)
    const mean = values.reduce((sum, v) => sum + v, 0) / values.length
    const variance = values.reduce((sum, v) => sum + (v - mean) ** 2, 0) / values.length
    expect(Math.abs(mean)).toBeLessThan(0.02)
    expect(Math.sqrt(variance)).toBeCloseTo(1, 1)
    expect(values.every(Number.isFinite)).toBe(true)
  })
  it('ne produit pas de valeur infinie quand le tirage uniforme vaut 0', () => {
    const next = standardNormal(() => 0)
    expect(Number.isFinite(next())).toBe(true)
  })
})
