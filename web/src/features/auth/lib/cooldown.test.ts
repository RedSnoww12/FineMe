import { describe, expect, it } from 'vitest'
import { formatWait, remainingSeconds } from './cooldown'

describe('formatWait', () => {
  it('affiche les secondes sous une minute, puis des minutes arrondies au-dessus', () => {
    expect(formatWait(0)).toBe('0 s')
    expect(formatWait(29.2)).toBe('30 s')
    expect(formatWait(59)).toBe('59 s')
    expect(formatWait(60)).toBe('1 min')
    expect(formatWait(61)).toBe('2 min')
    expect(formatWait(900)).toBe('15 min')
  })
})

describe('remainingSeconds', () => {
  it('compte les secondes restantes sans devenir négatif', () => {
    expect(remainingSeconds(null, 1_000)).toBe(0)
    expect(remainingSeconds(31_000, 1_000)).toBe(30)
    expect(remainingSeconds(31_000, 1_500)).toBe(30)
    expect(remainingSeconds(31_000, 30_999)).toBe(1)
    expect(remainingSeconds(31_000, 40_000)).toBe(0)
  })
})
