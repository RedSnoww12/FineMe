import { describe, expect, it } from 'vitest'
import { ACCOUNT_KIND_LABEL, ACCOUNT_KIND_ORDER, envelopeLabel, GOAL_STATUS } from './labels'

describe('envelopeLabel', () => {
  it('traduit les enveloppes connues', () => {
    expect(envelopeLabel('pea')).toBe('PEA')
    expect(envelopeLabel('assurance_vie')).toBe('Assurance-vie')
  })
  it('garde telle quelle une enveloppe libre', () => {
    expect(envelopeLabel('madelin')).toBe('madelin')
  })
  it('nomme l’absence d’enveloppe', () => {
    expect(envelopeLabel(null)).toBe('Sans enveloppe')
    expect(envelopeLabel('')).toBe('Sans enveloppe')
  })
})

describe('libellés du contrat', () => {
  it('ordonne tous les types de compte, dettes en dernier', () => {
    expect([...ACCOUNT_KIND_ORDER].sort()).toEqual(Object.keys(ACCOUNT_KIND_LABEL).sort())
    expect(ACCOUNT_KIND_ORDER.at(-1)).toBe('loan')
  })
  it('couvre tous les statuts d’objectif', () => {
    expect(Object.keys(GOAL_STATUS).sort()).toEqual(
      ['challenging', 'completed', 'funded', 'on_track', 'overdue', 'underfunded'].sort(),
    )
  })
})
