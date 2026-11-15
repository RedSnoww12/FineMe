import { describe, expect, it } from 'vitest'
import { parseDate, parseFrNumber } from './parse'

describe('parseFrNumber', () => {
  it.each([
    ['1 234,56', 1234.56],
    ['1 234,56', 1234.56],
    ['1 234,56 €', 1234.56],
    ['-45,90', -45.9],
    ['45,90-', -45.9],
    ['(12,50)', -12.5],
    ['+300', 300],
    ['1.234,56', 1234.56],
    ['1,234.56', 1234.56],
    ['1,234,567', 1234567],
    ['0,5', 0.5],
    ['12.5', 12.5],
    ['1234', 1234],
    ['−12,00', -12],
  ])('lit %s', (input, expected) => {
    expect(parseFrNumber(input)).toBeCloseTo(expected, 10)
  })

  it.each(['', '   ', 'abc', '12a', null, undefined, '-'])('rejette %s', (input) => {
    expect(parseFrNumber(input)).toBeNull()
  })
})

describe('parseDate', () => {
  it.each([
    ['31/12/2024', '2024-12-31'],
    ['1/2/2024', '2024-02-01'],
    ['05-03-2023', '2023-03-05'],
    ['05.03.23', '2023-03-05'],
    ['2024-06-09', '2024-06-09'],
    ['2024-06-09T10:00:00', '2024-06-09'],
    ['29/02/2024', '2024-02-29'],
  ])('lit %s', (input, expected) => {
    expect(parseDate(input)).toBe(expected)
  })

  it.each(['', '31/02/2024', '12/13/2024', 'hier', '2024-13-01', null])('rejette %s', (input) => {
    expect(parseDate(input)).toBeNull()
  })
})

