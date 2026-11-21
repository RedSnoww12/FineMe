import { afterEach, describe, expect, it } from 'vitest'
import {
  date,
  dateTime,
  duration,
  MASK,
  money,
  moneyCompact,
  number,
  pct,
  pctFromFraction,
  setAmountsHidden,
  shortDateTime,
  toInput,
  todayIso,
} from './format'

// fr-FR sépare les milliers par une espace fine insécable et place une espace insécable avant € et %.
const plain = (text: string) => text.replace(/[  ]/g, ' ')

describe('money', () => {
  it('formate en euros sans décimales par défaut', () => {
    expect(plain(money(1234567.89))).toBe('1 234 568 €')
    expect(plain(money(0))).toBe('0 €')
  })
  it('affiche les décimales demandées', () => {
    expect(plain(money(1234.5, { decimals: 2 }))).toBe('1 234,50 €')
  })
  it('signe les montants positifs sur demande, jamais zéro', () => {
    expect(plain(money(250, { signed: true }))).toBe('+250 €')
    expect(plain(money(-250, { signed: true }))).toBe('-250 €')
    expect(plain(money(0, { signed: true }))).toBe('0 €')
  })
  it('n’affiche pas de zéro négatif', () => {
    expect(plain(money(-0.2))).toBe('0 €')
    expect(plain(money(-0.004, { decimals: 2 }))).toBe('0,00 €')
  })
  it('utilise la devise du compte', () => {
    expect(plain(money(10, { currency: 'USD', decimals: 2 }))).toBe('10,00 $US')
  })
  it('ne plante pas sur un code devise inconnu', () => {
    expect(plain(money(10, { currency: 'pas-une-devise' }))).toBe('10')
  })
  it('remplace les valeurs absentes par un tiret', () => {
    expect(money(null)).toBe('–')
    expect(money(undefined)).toBe('–')
    expect(money(Number.NaN)).toBe('–')
    expect(money(Number.POSITIVE_INFINITY)).toBe('–')
  })
})

describe('mode discret', () => {
  afterEach(() => setAmountsHidden(false))

  it('masque les chiffres en gardant le symbole de la devise', () => {
    expect(plain(money(1234.5, { masked: true }))).toBe(`${MASK} €`)
    expect(plain(money(10, { currency: 'USD', decimals: 2, masked: true }))).toBe(`${MASK} $US`)
  })
  it('ne laisse deviner ni le signe ni l’ordre de grandeur', () => {
    const masked = [money(-250, { signed: true, masked: true }), money(250, { signed: true, masked: true }), money(12_345_678, { masked: true })]
    expect(new Set(masked).size).toBe(1)
    expect(masked[0]).not.toMatch(/[\d+-]/)
  })
  it('suit l’interrupteur global, y compris pour les axes de graphiques', () => {
    setAmountsHidden(true)
    expect(plain(money(1234))).toBe(`${MASK} €`)
    expect(plain(moneyCompact(12_000))).toBe(`${MASK} €`)
    setAmountsHidden(false)
    expect(plain(money(1234))).toBe('1 234 €')
  })
  it('garde le tiret des valeurs absentes et ne masque pas les pourcentages', () => {
    setAmountsHidden(true)
    expect(money(null)).toBe('–')
    expect(plain(pct(7))).toBe('7,0 %')
  })
  it('reste sur un code devise inconnu', () => {
    expect(money(10, { currency: 'pas-une-devise', masked: true })).toBe(MASK)
  })
})

describe('shortDateTime', () => {
  it('formate en JJ/MM HH:mm, heure locale', () => {
    expect(shortDateTime(new Date(2026, 10, 3, 9, 5).getTime())).toBe('03/11 09:05')
  })
})

describe('moneyCompact', () => {
  it('abrège les grands montants', () => {
    expect(plain(moneyCompact(12_000))).toBe('12 k €')
    expect(plain(moneyCompact(1_250_000))).toBe('1,3 M €')
    expect(plain(moneyCompact(0))).toBe('0 €')
  })
})

describe('pct', () => {
  it('formate un pourcentage déjà exprimé en points', () => {
    expect(plain(pct(7))).toBe('7,0 %')
    expect(plain(pct(3.456, 2))).toBe('3,46 %')
  })
  it('signe les valeurs positives sur demande', () => {
    expect(plain(pct(12.5, 1, true))).toBe('+12,5 %')
    expect(plain(pct(-4, 1, true))).toBe('-4,0 %')
    expect(plain(pct(0, 1, true))).toBe('0,0 %')
  })
  it('remplace les valeurs absentes par un tiret', () => {
    expect(pct(null)).toBe('–')
    expect(pct(Number.NaN)).toBe('–')
  })
})

describe('pctFromFraction', () => {
  it('convertit une part ou des frais exprimés en fraction', () => {
    expect(plain(pctFromFraction(0.5, 0))).toBe('50 %')
    expect(plain(pctFromFraction(0.002, 2))).toBe('0,20 %')
    expect(plain(pctFromFraction(1, 0))).toBe('100 %')
  })
  it('remplace les valeurs absentes par un tiret', () => {
    expect(pctFromFraction(null)).toBe('–')
    expect(pctFromFraction(undefined)).toBe('–')
  })
})

describe('number', () => {
  it('groupe les milliers et limite les décimales', () => {
    expect(plain(number(1234.56789))).toBe('1 234,5679')
    expect(plain(number(12.5, 0))).toBe('13')
    expect(number(null)).toBe('–')
  })
})

describe('date', () => {
  it('formate une date du contrat sans décalage de fuseau', () => {
    expect(date('2024-12-31')).toBe('31/12/2024')
    expect(date('2024-01-01')).toBe('01/01/2024')
  })
  it('propose un format long et un format mois', () => {
    expect(date('2030-06-30', 'long')).toBe('30 juin 2030')
    expect(date('2030-06-30', 'month')).toBe('juin 2030')
  })
  it('remplace les dates absentes ou illisibles par un tiret', () => {
    expect(date(null)).toBe('–')
    expect(date('')).toBe('–')
    expect(date('pas une date')).toBe('–')
    expect(dateTime(null)).toBe('–')
    expect(dateTime('pas une date')).toBe('–')
  })
  it('formate une date et heure ISO', () => {
    expect(dateTime('2025-03-01T09:00:00')).toContain('2025')
  })
})

describe('duration', () => {
  it.each([
    [0, '0 mois'],
    [5, '5 mois'],
    [12, '1 an'],
    [24, '2 ans'],
    [171, '14 ans et 3 mois'],
  ])('exprime %i mois', (months, expected) => {
    expect(duration(months)).toBe(expected)
  })
})

describe('toInput', () => {
  it('produit une valeur ressaisissable, virgule décimale et sans séparateur de milliers', () => {
    expect(toInput(1234.5)).toBe('1234,5')
    expect(toInput(45000, 0)).toBe('45000')
    expect(toInput(0.31, 3)).toBe('0,31')
  })
  it('renvoie une chaîne vide pour une valeur absente', () => {
    expect(toInput(null)).toBe('')
    expect(toInput(undefined)).toBe('')
    expect(toInput(Number.NaN)).toBe('')
  })
})

describe('todayIso', () => {
  it('renvoie une date au format du contrat', () => {
    expect(todayIso()).toMatch(/^\d{4}-\d{2}-\d{2}$/)
  })
})
