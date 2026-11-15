import { useState, type FormEvent } from 'react'
import { useSaveHolding } from '../hooks/useAccountsApi'
import type { Holding, HoldingKind } from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'
import { toInput } from '@/shared/lib/format'
import { HOLDING_KIND_LABEL, SECURITY_TYPES } from '@/shared/lib/labels'
import { Button, FormError, Modal, NumberField, SelectField, TextField } from '@/shared/ui/ui'

type Props = {
  accountId: string
  accountCurrency: string
  defaultKind: HoldingKind
  holding?: Holding
  onClose: () => void
}

type Errors = Partial<Record<'name' | 'quantity' | 'buy_price' | 'current_price' | 'currency' | 'expense_ratio' | 'isin', string>>

export function HoldingFormModal({ accountId, accountCurrency, defaultKind, holding, onClose }: Props) {
  const [form, setForm] = useState({
    kind: holding?.kind ?? defaultKind,
    name: holding?.name ?? '',
    isin: holding?.isin ?? '',
    symbol: holding?.symbol ?? '',
    security_type: holding?.security_type ?? '',
    quantity: toInput(holding?.quantity, 8),
    buy_price: toInput(holding?.buy_price, 6),
    current_price: toInput(holding?.current_price, 6),
    currency: holding?.currency ?? accountCurrency,
    expense_ratio: holding?.expense_ratio != null ? toInput(holding.expense_ratio * 100, 4) : '',
  })
  const [errors, setErrors] = useState<Errors>({})
  const set = (key: keyof typeof form) => (value: string) => setForm((f) => ({ ...f, [key]: value }))

  const mutation = useSaveHolding(accountId, holding?.id, onClose)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: Errors = {}
    const quantity = parseFrNumber(form.quantity)
    const currentPrice = parseFrNumber(form.current_price)
    const buyPrice = form.buy_price.trim() ? parseFrNumber(form.buy_price) : null
    const expense = form.expense_ratio.trim() ? parseFrNumber(form.expense_ratio) : null
    const currency = form.currency.trim().toUpperCase()
    const isin = form.isin.trim().toUpperCase()

    if (!form.name.trim()) next.name = 'Donnez un nom à cette position.'
    if (quantity === null || quantity < 0) next.quantity = 'Saisissez une quantité, par exemple 12,5.'
    if (currentPrice === null || currentPrice < 0) next.current_price = 'Saisissez le prix unitaire actuel.'
    if (form.buy_price.trim() && (buyPrice === null || buyPrice < 0)) next.buy_price = 'Saisissez un prix, ou laissez vide.'
    if (form.expense_ratio.trim() && (expense === null || expense < 0 || expense > 100))
      next.expense_ratio = 'Saisissez un pourcentage, par exemple 0,25.'
    if (!/^[A-Z]{3}$/.test(currency)) next.currency = 'Code sur 3 lettres, par exemple EUR.'
    if (isin && !/^[A-Z]{2}[A-Z0-9]{9}\d$/.test(isin)) next.isin = 'Un code ISIN compte 12 caractères, par exemple FR0011869353.'
    setErrors(next)
    if (Object.keys(next).length > 0) return

    mutation.mutate({
      kind: form.kind,
      name: form.name.trim(),
      isin: isin || null,
      symbol: form.symbol.trim().toUpperCase() || null,
      security_type: form.kind === 'security' ? form.security_type || null : null,
      quantity: quantity as number,
      buy_price: buyPrice,
      current_price: currentPrice as number,
      currency,
      expense_ratio: expense === null ? null : expense / 100,
    })
  }

  const isFondsEuro = form.kind === 'fonds_euro'

  return (
    <Modal
      title={holding ? 'Modifier la position' : 'Ajouter une position'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="holding-form" busy={mutation.isPending}>
            {holding ? 'Enregistrer' : 'Ajouter'}
          </Button>
        </>
      }
    >
      <form id="holding-form" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <SelectField
            label="Nature"
            value={form.kind}
            onChange={(value) => setForm((f) => ({ ...f, kind: value as HoldingKind }))}
            options={Object.entries(HOLDING_KIND_LABEL).map(([value, label]) => ({ value, label }))}
          />
          {form.kind === 'security' ? (
            <SelectField
              label="Type de titre"
              value={form.security_type}
              onChange={set('security_type')}
              options={[
                { value: '', label: 'Non précisé' },
                ...SECURITY_TYPES,
                ...(form.security_type && !SECURITY_TYPES.some((t) => t.value === form.security_type)
                  ? [{ value: form.security_type, label: form.security_type }]
                  : []),
              ]}
            />
          ) : null}
        </div>
        <TextField label="Nom" value={form.name} onChange={set('name')} error={errors.name} />
        {!isFondsEuro ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <TextField
              label="Code ISIN"
              value={form.isin}
              onChange={set('isin')}
              error={errors.isin}
              hint="Facultatif. Sert à actualiser le cours."
              autoCapitalize="characters"
            />
            <TextField
              label={form.kind === 'crypto' ? 'Code crypto' : 'Symbole'}
              value={form.symbol}
              onChange={set('symbol')}
              hint={form.kind === 'crypto' ? 'Par exemple BTC.' : 'Facultatif, par exemple CW8.'}
              autoCapitalize="characters"
            />
          </div>
        ) : null}
        <div className="grid gap-4 sm:grid-cols-2">
          <NumberField
            label="Quantité"
            value={form.quantity}
            onChange={set('quantity')}
            error={errors.quantity}
            hint={isFondsEuro ? 'Pour un fonds en euros : 1, avec l’encours comme prix.' : undefined}
          />
          <TextField
            label="Devise"
            value={form.currency}
            onChange={set('currency')}
            error={errors.currency}
            maxLength={3}
            autoCapitalize="characters"
          />
          <NumberField
            label="Prix unitaire actuel"
            value={form.current_price}
            onChange={set('current_price')}
            error={errors.current_price}
          />
          <NumberField
            label="Prix de revient unitaire"
            value={form.buy_price}
            onChange={set('buy_price')}
            error={errors.buy_price}
            hint="Facultatif. Sert à calculer la plus-value."
          />
        </div>
        <NumberField
          label="Frais courants annuels"
          value={form.expense_ratio}
          onChange={set('expense_ratio')}
          error={errors.expense_ratio}
          suffix="%"
          hint="Facultatif. Figure dans le document d’informations clés du fonds."
        />
        <FormError error={mutation.error} />
      </form>
    </Modal>
  )
}
