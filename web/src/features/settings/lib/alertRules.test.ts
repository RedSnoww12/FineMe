import { describe, expect, it } from 'vitest'
import type { AlertRule } from '@/shared/api/types'
import { readRuleForm } from './alertRules'

const ceiling: AlertRule = { kind: 'savings_ceiling', enabled: true, threshold_pct: 90, ceiling: 10_000 }
const change: AlertRule = { kind: 'net_worth_change', enabled: true, threshold_pct: 10, ceiling: null }
const goals: AlertRule = { kind: 'goals', enabled: true, threshold_pct: null, ceiling: null }

describe('réglages des alertes', () => {
  it('lit les seuils au format français', () => {
    expect(readRuleForm(ceiling, { threshold: '85,5', ceiling: '12 000' })).toEqual({
      input: { threshold_pct: 85.5, ceiling: 12_000 },
      errors: null,
    })
    expect(readRuleForm(change, { threshold: '250', ceiling: '' }).input).toEqual({ threshold_pct: 250 })
  })

  it('refuse les seuils hors bornes, selon la règle', () => {
    for (const threshold of ['0', '-5', '100,5', 'abc', '']) {
      expect(readRuleForm(ceiling, { threshold, ceiling: '1000' }).errors?.threshold, threshold).toBeTruthy()
    }
    expect(readRuleForm(ceiling, { threshold: '100', ceiling: '1000' }).errors).toBeNull()
    expect(readRuleForm(change, { threshold: '1001', ceiling: '' }).errors?.threshold).toMatch(/1\s000/)
    expect(readRuleForm(ceiling, { threshold: '90', ceiling: '0' }).errors?.ceiling).toBeTruthy()
  })

  it('n’envoie rien pour une règle sans seuil', () => {
    expect(readRuleForm(goals, { threshold: 'ignoré', ceiling: 'ignoré' })).toEqual({ input: {}, errors: null })
  })
})
