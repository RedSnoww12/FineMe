import { describe, expect, it } from 'vitest'
import {
  categoryLabel,
  cleanCategory,
  isEmptyMonth,
  isMonth,
  monthLabel,
  parseTarget,
  shiftMonth,
  sourceLabel,
  targetStatus,
  transactionPatch,
  transferChoice,
} from './budget'

describe('mois', () => {
  it('se décale à travers les années', () => {
    expect(shiftMonth('2026-01', -1)).toBe('2025-12')
    expect(shiftMonth('2025-12', 1)).toBe('2026-01')
    expect(shiftMonth('2026-03', -14)).toBe('2025-01')
  })

  it('se lit et s’affiche en français', () => {
    expect(isMonth('2026-12')).toBe(true)
    expect(isMonth('2026-13')).toBe(false)
    expect(isMonth('')).toBe(false)
    expect(monthLabel('2026-02')).toBe('février 2026')
  })
})

describe('cibles', () => {
  it('suit la consommation de la cible', () => {
    expect(targetStatus({ category: 'Loisirs', amount: 50, target: null })).toBeNull()
    expect(targetStatus({ category: 'Loisirs', amount: 50, target: 200 })).toEqual({ pct: 25, remaining: 150, over: false })
    expect(targetStatus({ category: 'Loisirs', amount: 250, target: 200 })).toEqual({ pct: 125, remaining: -50, over: true })
  })

  it('traite une cible nulle comme dépassée dès la première dépense', () => {
    expect(targetStatus({ category: 'Tabac', amount: 0, target: 0 })).toEqual({ pct: 0, remaining: 0, over: false })
    expect(targetStatus({ category: 'Tabac', amount: 5, target: 0 })?.over).toBe(true)
  })

  it('lit la saisie d’une cible', () => {
    expect(parseTarget(' ')).toEqual({ value: null })
    expect(parseTarget('1 250,50')).toEqual({ value: 1250.5 })
    expect(parseTarget('abc')).toEqual({ error: 'Montant invalide.' })
    expect(parseTarget('-3')).toEqual({ error: 'Le montant ne peut pas être négatif.' })
  })
})

describe('catégories', () => {
  it('nomme les catégories et leur origine', () => {
    expect(categoryLabel(null)).toBe('Non catégorisé')
    expect(categoryLabel('Logement')).toBe('Logement')
    expect(sourceLabel('rule')).toBe('Règle')
    expect(sourceLabel(null)).toBe('Aucune règle')
  })

  it('nettoie la catégorie saisie', () => {
    expect(cleanCategory('  Vie   courante ')).toBe('Vie courante')
    expect(cleanCategory('   ')).toBeNull()
  })

  it('n’envoie que les choix qui changent', () => {
    const auto = { category: 'Alimentation', category_source: 'default' as const, internal_transfer: false, internal_transfer_manual: false }
    expect(transactionPatch(auto, { category: 'Alimentation', transfer: 'auto' })).toEqual({})
    expect(transactionPatch(auto, { category: ' Courses ', transfer: 'yes' })).toEqual({ category: 'Courses', internal_transfer: true })
    // Vider la catégorie automatique ne crée pas de choix manuel.
    expect(transactionPatch(auto, { category: '', transfer: 'auto' })).toEqual({})
    const manual = { ...auto, category_source: 'manual' as const, internal_transfer: true, internal_transfer_manual: true }
    expect(transferChoice(manual)).toBe('yes')
    expect(transactionPatch(manual, { category: '', transfer: 'auto' })).toEqual({ category: null, internal_transfer: null })
    expect(transactionPatch(manual, { category: 'Alimentation', transfer: 'no' })).toEqual({ internal_transfer: false })
  })

  it('reconnaît un mois sans flux', () => {
    const flows = { month: '2026-01', income: 0, expenses: 0, savings: 0, savings_rate: null }
    expect(isEmptyMonth(flows, [{ category: 'Loisirs', amount: 0, target: 100 }])).toBe(true)
    expect(isEmptyMonth({ ...flows, income: 10 }, [])).toBe(false)
  })
})
