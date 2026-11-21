import { useState, type FormEvent } from 'react'
import { useCreateOperation, useDeleteOperation, useOperations, useOperationsSummary } from '../hooks/useAccountsApi'
import {
  acceptsHolding,
  buildOperationInput,
  cashEffect,
  emptyOperationForm,
  isTrade,
  OPERATION_KIND_LABEL,
  OPERATION_KINDS,
  type OperationErrors,
  type OperationForm,
} from '../lib/operations'
import type { AccountDetail, Operation, OperationKind } from '@/shared/api/types'
import { date, money, number, todayIso } from '@/shared/lib/format'
import {
  Button,
  ConfirmModal,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Modal,
  Notice,
  NumberField,
  Panel,
  SelectField,
  Signed,
  TextField,
} from '@/shared/ui/ui'

function OperationFormModal({ account, onClose }: { account: AccountDetail; onClose: () => void }) {
  const today = todayIso()
  const [form, setForm] = useState<OperationForm>(() => emptyOperationForm(today))
  const [errors, setErrors] = useState<OperationErrors>({})
  const set = (key: keyof OperationForm) => (value: string) => setForm((f) => ({ ...f, [key]: value }))
  const mutation = useCreateOperation(account.id, onClose)
  const trade = isTrade(form.kind)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const result = buildOperationInput(form, today)
    setErrors(result.errors ?? {})
    if (result.input) mutation.mutate(result.input)
  }

  const holdingOptions = [
    ...(trade ? [{ value: '', label: 'Choisir une position' }] : [{ value: '', label: 'Aucune' }]),
    ...account.holdings.map((holding) => ({ value: holding.id, label: holding.name })),
  ]

  return (
    <Modal
      title="Ajouter une opération"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="operation-form" busy={mutation.isPending}>
            Ajouter
          </Button>
        </>
      }
    >
      <form id="operation-form" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <SelectField
            label="Type d’opération"
            value={form.kind}
            onChange={(value) => setForm((f) => ({ ...f, kind: value as OperationKind }))}
            options={OPERATION_KINDS.map((kind) => ({ value: kind, label: OPERATION_KIND_LABEL[kind] }))}
          />
          <TextField label="Date" type="date" value={form.date} max={today} onChange={set('date')} error={errors.date} />
        </div>
        {acceptsHolding(form.kind) ? (
          <SelectField
            label="Position"
            value={form.holding_id}
            onChange={set('holding_id')}
            options={holdingOptions}
            error={errors.holding_id}
            hint={
              account.holdings.length === 0
                ? 'Ajoutez d’abord la position dans l’onglet Positions (quantité 0).'
                : trade
                  ? undefined
                  : 'Facultatif.'
            }
          />
        ) : null}
        {trade ? (
          <div className="grid gap-4 sm:grid-cols-3">
            <NumberField label="Quantité" value={form.quantity} onChange={set('quantity')} error={errors.quantity} />
            <NumberField label="Cours unitaire" value={form.unit_price} onChange={set('unit_price')} error={errors.unit_price} />
            <NumberField label="Frais" value={form.fees} onChange={set('fees')} error={errors.fees} hint="Courtage, taxes." />
          </div>
        ) : (
          <NumberField label="Montant" value={form.amount} onChange={set('amount')} error={errors.amount} />
        )}
        <TextField label="Note" value={form.note} onChange={set('note')} hint="Facultatif." />
        <FormError error={mutation.error} />
      </form>
    </Modal>
  )
}

function YearlySummary({ accountId }: { accountId: string }) {
  const summary = useOperationsSummary(accountId)
  if (summary.isPending) return <LoadingBlock rows={1} label="Chargement de la synthèse…" />
  if (summary.isError) return <ErrorState error={summary.error} onRetry={() => void summary.refetch()} />
  if (summary.data.years.length === 0) return null
  return (
    <div className="table-wrap">
      <table className="table" aria-label="Synthèse annuelle">
        <thead>
          <tr>
            <th>Année</th>
            <th className="r">Plus-values réalisées</th>
            <th className="r">Dividendes</th>
            <th className="r">Frais</th>
            <th className="r">Versements</th>
            <th className="r">Retraits</th>
          </tr>
        </thead>
        <tbody>
          {summary.data.years.map((year) => (
            <tr key={year.year}>
              <td className="num font-medium">{year.year}</td>
              <td className="r">
                <Signed value={year.realized_gain}>{money(year.realized_gain, { decimals: 2, signed: true })}</Signed>
              </td>
              <td className="r">{money(year.dividends, { decimals: 2 })}</td>
              <td className="r">{money(year.fees, { decimals: 2 })}</td>
              <td className="r">{money(year.deposits, { decimals: 2 })}</td>
              <td className="r">{money(year.withdrawals, { decimals: 2 })}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function describe(operation: Operation): string {
  if (operation.quantity !== null && operation.unit_price !== null) {
    return `${number(operation.quantity, 6)} × ${money(operation.unit_price, { decimals: 2, currency: operation.currency })}`
  }
  return ''
}

/** Journal d'opérations d'un compte d'investissement, avec sa synthèse annuelle. */
export function OperationsJournal({ account }: { account: AccountDetail }) {
  const operations = useOperations(account.id)
  const [adding, setAdding] = useState(false)
  const [deleting, setDeleting] = useState<Operation | null>(null)
  const remove = useDeleteOperation(() => setDeleting(null))

  return (
    <Panel
      title="Journal d’opérations"
      flush
      action={
        <Button size="sm" variant="primary" icon="plus" onClick={() => setAdding(true)}>
          Ajouter une opération
        </Button>
      }
    >
      <div className="flex flex-col gap-3 px-4 pt-2 pb-4 sm:px-5">
        <Notice>
          Une position qui a des achats ou des ventes dans ce journal voit sa quantité et son prix de revient (PRU moyen
          pondéré, frais inclus) recalculés automatiquement. Le solde espèces du compte n’est pas modifié.
        </Notice>
        {operations.isPending ? (
          <LoadingBlock rows={2} />
        ) : operations.isError ? (
          <ErrorState error={operations.error} onRetry={() => void operations.refetch()} />
        ) : operations.data.length === 0 ? (
          <p className="text-muted">
            Aucune opération. Enregistrez vos achats, ventes, dividendes et versements pour suivre vos plus-values réalisées.
          </p>
        ) : (
          <YearlySummary accountId={account.id} />
        )}
      </div>

      {operations.data && operations.data.length > 0 ? (
        <div className="table-wrap">
          <table className="table" aria-label="Opérations">
            <thead>
              <tr>
                <th>Date</th>
                <th>Opération</th>
                <th className="r">Détail</th>
                <th className="r">Mouvement d’espèces</th>
                <th className="r">Plus-value réalisée</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {operations.data.map((operation) => {
                const label = `${OPERATION_KIND_LABEL[operation.kind]}${operation.holding_name ? ` ${operation.holding_name}` : ''}`
                return (
                  <tr key={operation.id}>
                    <td className="num">{date(operation.date)}</td>
                    <td className="min-w-[160px]">
                      <div className="font-medium">{label}</div>
                      {operation.note ? <div className="text-[13px] text-muted">{operation.note}</div> : null}
                    </td>
                    <td className="r num">
                      {describe(operation)}
                      {operation.fees > 0 ? (
                        <div className="text-[12.5px] text-muted">
                          frais {money(operation.fees, { decimals: 2, currency: operation.currency })}
                        </div>
                      ) : null}
                    </td>
                    <td className="r">
                      <Signed value={cashEffect(operation)}>
                        {money(cashEffect(operation), { decimals: 2, currency: operation.currency, signed: true })}
                      </Signed>
                    </td>
                    <td className="r">
                      {operation.realized_gain !== null ? (
                        <Signed value={operation.realized_gain}>
                          {money(operation.realized_gain, { decimals: 2, currency: operation.currency, signed: true })}
                        </Signed>
                      ) : (
                        '–'
                      )}
                    </td>
                    <td>
                      <div className="flex justify-end">
                        <IconButton
                          icon="trash"
                          label={`Supprimer l’opération ${label} du ${date(operation.date)}`}
                          onClick={() => {
                            remove.reset()
                            setDeleting(operation)
                          }}
                        />
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      ) : null}

      {adding ? <OperationFormModal account={account} onClose={() => setAdding(false)} /> : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer cette opération ?"
          confirmLabel="Supprimer l’opération"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          {OPERATION_KIND_LABEL[deleting.kind]} du {date(deleting.date)} : la position concernée sera recalculée.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}
