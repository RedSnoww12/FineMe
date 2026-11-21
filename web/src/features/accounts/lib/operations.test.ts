import { describe, expect, it } from 'vitest'
import {
  buildOperationInput,
  cashEffect,
  emptyOperationForm,
  operationToForm,
  type OperationForm,
} from '@/features/accounts/lib/operations'
import type { Operation } from '@/shared/api/types'

const TODAY = '2025-06-30'

function form(overrides: Partial<OperationForm>): OperationForm {
  return { ...emptyOperationForm(TODAY), ...overrides }
}

describe('buildOperationInput', () => {
  it('construit un achat avec ses frais, en lisant les nombres à la française', () => {
    const result = buildOperationInput(
      form({ kind: 'buy', holding_id: 'h-1', quantity: '12,5', unit_price: '1 234,50', fees: '1,99', note: '  Ordre  ' }),
      TODAY,
    )
    expect(result.input).toEqual({
      kind: 'buy',
      date: TODAY,
      holding_id: 'h-1',
      quantity: 12.5,
      unit_price: 1234.5,
      fees: 1.99,
      note: 'Ordre',
    })
  })

  it('met les frais à zéro quand ils sont laissés vides', () => {
    const result = buildOperationInput(form({ kind: 'sell', holding_id: 'h-1', quantity: '2', unit_price: '10' }), TODAY)
    expect(result.input).toMatchObject({ kind: 'sell', fees: 0 })
  })

  it('exige la position, une quantité positive et le cours pour un achat ou une vente', () => {
    const result = buildOperationInput(form({ kind: 'sell', quantity: '0', unit_price: 'abc', fees: '-1' }), TODAY)
    expect(result.errors).toEqual({
      holding_id: 'Choisissez la position achetée ou vendue.',
      quantity: 'Saisissez une quantité positive, par exemple 12,5.',
      unit_price: 'Saisissez le cours unitaire.',
      fees: 'Saisissez les frais, ou laissez vide.',
    })
  })

  it('n’envoie que le montant pour un versement, sans position', () => {
    const result = buildOperationInput(
      form({ kind: 'deposit', holding_id: 'h-1', quantity: '3', amount: '500' }),
      TODAY,
    )
    expect(result.input).toEqual({ kind: 'deposit', date: TODAY, amount: 500 })
  })

  it('garde la position facultative d’un dividende', () => {
    expect(buildOperationInput(form({ kind: 'dividend', holding_id: 'h-1', amount: '12,40' }), TODAY).input).toEqual({
      kind: 'dividend',
      date: TODAY,
      holding_id: 'h-1',
      amount: 12.4,
    })
    expect(buildOperationInput(form({ kind: 'fee', amount: '3' }), TODAY).input).toEqual({
      kind: 'fee',
      date: TODAY,
      amount: 3,
    })
  })

  it('refuse un montant nul et une date future ou absente', () => {
    expect(buildOperationInput(form({ kind: 'withdrawal', amount: '0' }), TODAY).errors).toEqual({
      amount: 'Saisissez un montant positif.',
    })
    expect(buildOperationInput(form({ kind: 'fee', amount: '1', date: '2025-07-01' }), TODAY).errors).toEqual({
      date: 'La date ne peut pas être dans le futur.',
    })
    expect(buildOperationInput(form({ kind: 'fee', amount: '1', date: '' }), TODAY).errors).toEqual({
      date: 'Saisissez la date de l’opération.',
    })
  })
})

describe('cashEffect', () => {
  it('signe chaque type d’opération du point de vue des espèces', () => {
    expect(cashEffect({ kind: 'buy', amount: 1000, fees: 5 })).toBe(-1005)
    expect(cashEffect({ kind: 'sell', amount: 1200, fees: 4 })).toBe(1196)
    expect(cashEffect({ kind: 'dividend', amount: 12, fees: 0 })).toBe(12)
    expect(cashEffect({ kind: 'deposit', amount: 500, fees: 0 })).toBe(500)
    expect(cashEffect({ kind: 'fee', amount: 3, fees: 0 })).toBe(-3)
    expect(cashEffect({ kind: 'withdrawal', amount: 200, fees: 0 })).toBe(-200)
  })
})

function operation(overrides: Partial<Operation>): Operation {
  return {
    id: 'op-1',
    account_id: 'a-1',
    kind: 'buy',
    date: '2025-01-10',
    holding_id: 'h-1',
    holding_name: 'Fonds fictif',
    quantity: 12.5,
    unit_price: 1234.5,
    amount: 15431.25,
    fees: 1.99,
    currency: 'EUR',
    note: null,
    realized_gain: null,
    ...overrides,
  }
}

describe('operationToForm', () => {
  it('préremplit un achat avec des nombres à la française, sans le montant calculé', () => {
    expect(operationToForm(operation({ note: 'Ordre' }))).toEqual({
      kind: 'buy',
      date: '2025-01-10',
      holding_id: 'h-1',
      quantity: '12,5',
      unit_price: '1234,5',
      fees: '1,99',
      amount: '',
      note: 'Ordre',
    })
  })

  it('laisse les frais vides quand ils sont nuls et garde les petites quantités lisibles', () => {
    const form = operationToForm(operation({ kind: 'sell', quantity: 0.0000005, fees: 0 }))
    expect(form.fees).toBe('')
    expect(form.quantity).toBe('0,0000005')
  })

  it('préremplit le montant d’un versement, sans position', () => {
    const form = operationToForm(
      operation({ kind: 'deposit', holding_id: null, quantity: null, unit_price: null, amount: 3000, fees: 0 }),
    )
    expect(form).toMatchObject({ kind: 'deposit', holding_id: '', quantity: '', unit_price: '', fees: '', amount: '3000' })
  })

  it('redonne le même corps que l’opération enregistrée quand rien n’est modifié', () => {
    const op = operation({ kind: 'dividend', quantity: null, unit_price: null, amount: 12.34, fees: 0, note: 'Coupon' })
    expect(buildOperationInput(operationToForm(op), TODAY).input).toEqual({
      kind: 'dividend',
      date: '2025-01-10',
      holding_id: 'h-1',
      amount: 12.34,
      note: 'Coupon',
    })
  })
})
