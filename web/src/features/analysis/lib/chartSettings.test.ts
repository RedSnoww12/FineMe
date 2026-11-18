import { describe, expect, it } from 'vitest'
import { canUseLog, DEFAULT_SETTINGS, movingAverage, parseSettings, smoothRows, toggleHidden } from './chartSettings'

describe('moyenne glissante', () => {
  it('moyenne les derniers jours de la fenêtre', () => {
    expect(movingAverage([1, 2, 3, 4, 5], 3)).toEqual([1, 1.5, 2, 3, 4])
  })

  it('porte sur les jours disponibles en début de série', () => {
    expect(movingAverage([10, 20], 7)).toEqual([10, 15])
  })

  it('ignore les valeurs manquantes et laisse un trou là où il n’y a pas de valeur', () => {
    expect(movingAverage([null, 2, null, 4], 3)).toEqual([null, 2, null, 3])
  })

  it('rend la série telle quelle avec une fenêtre d’un jour', () => {
    expect(movingAverage([3, null, 5], 1)).toEqual([3, null, 5])
  })

  it('lisse seulement les colonnes demandées', () => {
    const rows = [
      { date: 'a', x: 2, y: 1 },
      { date: 'b', x: 4, y: 9 },
    ]
    expect(smoothRows(rows, ['x'], 2)).toEqual([
      { date: 'a', x: 2, y: 1 },
      { date: 'b', x: 3, y: 9 },
    ])
  })
})

describe('échelle logarithmique', () => {
  it('est possible quand toutes les valeurs tracées sont positives, trous compris', () => {
    expect(canUseLog([{ a: 1 }, { a: null }, { a: 5 }], ['a'])).toBe(true)
  })

  it('est impossible avec une valeur nulle ou négative', () => {
    expect(canUseLog([{ a: 1 }, { a: 0 }], ['a'])).toBe(false)
    expect(canUseLog([{ a: -3 }], ['a'])).toBe(false)
  })

  it('ne regarde que les séries affichées, et refuse quand aucune ne l’est', () => {
    const rows = [{ a: 1, b: 0 }]
    expect(canUseLog(rows, ['a'])).toBe(true)
    expect(canUseLog([{ a: 1 }], [])).toBe(false)
  })
})

describe('réglages mémorisés', () => {
  it('relit des réglages valides', () => {
    expect(parseSettings('{"hidden":["real_estate"],"smooth":true,"log":false}')).toEqual({ hidden: ['real_estate'], smooth: true, log: false })
  })

  it('reprend les valeurs par défaut si rien n’est mémorisé ou si le contenu est invalide', () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS)
    expect(parseSettings('pas du json')).toEqual(DEFAULT_SETTINGS)
    expect(parseSettings('{"hidden":[1,"crypto"],"smooth":"oui"}')).toEqual({ hidden: ['crypto'], smooth: false, log: false })
  })

  it('masque puis réaffiche une série', () => {
    expect(toggleHidden(['a'], 'b')).toEqual(['a', 'b'])
    expect(toggleHidden(['a', 'b'], 'a')).toEqual(['b'])
  })
})
