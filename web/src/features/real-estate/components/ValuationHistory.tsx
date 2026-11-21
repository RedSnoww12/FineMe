import { useId, useMemo, useState, type FormEvent } from 'react'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { RealEstateItem, Valuation, ValuationInput } from '@/shared/api/types'
import { date, money, moneyCompact, number, todayIso } from '@/shared/lib/format'
import { parseFrNumber } from '@/shared/lib/parse'
import { newestFirst, valuationSeries, valueFromPricePerM2 } from '@/shared/lib/valuation'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import {
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  NumberField,
  Segmented,
  SelectField,
  TextField,
} from '@/shared/ui/ui'
import { useAddValuation, useDeleteValuation, useLinkedSchedules, useValuations } from '../hooks/useRealEstateApi'

const SOURCE_LABEL: Record<Valuation['source'], string> = {
  manual: 'Saisie',
  price_per_m2: 'Prix au m²',
  external: 'Estimation externe',
}

const SERIES = [
  { key: 'value', label: 'Valeur estimée', color: 'var(--c2)' },
  { key: 'net', label: 'Valeur nette de dette', color: 'var(--c1)' },
] as const

type Mode = 'value' | 'price'

function ValuationForm({
  accountId,
  surface,
  onDone,
}: {
  accountId: string
  surface: number | null
  onDone: () => void
}) {
  const today = todayIso()
  const [mode, setMode] = useState<Mode>('value')
  const [valuedOn, setValuedOn] = useState(today)
  const [amount, setAmount] = useState('')
  const [source, setSource] = useState<'manual' | 'external'>('manual')
  const [note, setNote] = useState('')
  const [error, setError] = useState<string | null>(null)
  const add = useAddValuation(accountId, onDone)

  const parsed = parseFrNumber(amount)
  const estimate = mode === 'price' ? valueFromPricePerM2(surface, parsed) : null
  const priceHint =
    mode !== 'price'
      ? undefined
      : !surface
        ? 'Renseignez la surface du bien pour estimer au prix du m².'
        : estimate !== null && parsed !== null
          ? `${number(surface, 2)} m² × ${money(parsed)} = ${money(estimate)}`
          : `Valeur = ${number(surface, 2)} m² × prix au m².`

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (parsed === null || parsed <= 0) {
      setError(mode === 'price' ? 'Indiquez un prix au m² strictement positif.' : 'Indiquez une valeur strictement positive.')
      return
    }
    if (!valuedOn || valuedOn > today) {
      setError('La date de l’estimation ne peut pas être dans le futur.')
      return
    }
    setError(null)
    const input: ValuationInput =
      mode === 'price'
        ? { price_per_m2: parsed, valued_on: valuedOn, note: note.trim() || null }
        : { value: parsed, source, valued_on: valuedOn, note: note.trim() || null }
    add.mutate(input)
  }

  return (
    <form onSubmit={submit} className="mt-3 flex flex-col gap-3 rounded-[12px] border border-line p-3" aria-label="Nouvelle estimation">
      <Segmented
        label="Méthode d’estimation"
        value={mode}
        onChange={setMode}
        options={[
          { value: 'value', label: 'Montant' },
          { value: 'price', label: 'Prix au m²' },
        ]}
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField label="Date de l’estimation" type="date" max={today} value={valuedOn} onChange={setValuedOn} required />
        <NumberField
          label={mode === 'price' ? 'Prix au m²' : 'Valeur estimée'}
          suffix={mode === 'price' ? '€/m²' : '€'}
          value={amount}
          onChange={setAmount}
          hint={priceHint}
          disabled={mode === 'price' && !surface}
        />
        {mode === 'value' ? (
          <SelectField
            label="Source"
            value={source}
            onChange={(value) => setSource(value === 'external' ? 'external' : 'manual')}
            options={[
              { value: 'manual', label: SOURCE_LABEL.manual },
              { value: 'external', label: SOURCE_LABEL.external },
            ]}
          />
        ) : null}
        <TextField label="Note (facultative)" value={note} onChange={setNote} maxLength={200} />
      </div>
      <FormError error={error ?? add.error} />
      <div className="flex gap-2">
        <Button type="submit" variant="primary" size="sm" busy={add.isPending}>
          Enregistrer l’estimation
        </Button>
        <Button size="sm" onClick={onDone}>
          Annuler
        </Button>
      </div>
    </form>
  )
}

/** Historique des estimations d'un bien : courbe valeur / valeur nette, liste, ajout et suppression. */
export function ValuationHistory({ item }: { item: RealEstateItem }) {
  const accountId = item.account.id
  const valuations = useValuations(accountId)
  const schedules = useLinkedSchedules(item.loans.map((loan) => loan.account_id))
  const [adding, setAdding] = useState(false)
  const [deleting, setDeleting] = useState<Valuation | null>(null)
  const remove = useDeleteValuation(() => setDeleting(null))
  const tableId = useId()

  const list = useMemo(() => newestFirst(valuations.data ?? []), [valuations.data])
  const points = useMemo(
    () => (schedules.data ? valuationSeries(list, schedules.data, todayIso()) : []),
    [list, schedules.data],
  )

  if (valuations.isPending || (item.loans.length > 0 && schedules.isPending)) {
    return <LoadingBlock rows={2} label="Chargement de l’historique…" />
  }
  if (valuations.isError || schedules.error) {
    return (
      <ErrorState
        error={valuations.error ?? schedules.error}
        onRetry={() => {
          void valuations.refetch()
          schedules.refetch()
        }}
      />
    )
  }

  const addButton = adding ? null : (
    <Button size="sm" icon="plus" onClick={() => setAdding(true)}>
      Ajouter une estimation
    </Button>
  )
  const form = adding ? (
    <ValuationForm accountId={accountId} surface={item.property.surface_m2} onDone={() => setAdding(false)} />
  ) : null

  if (list.length === 0) {
    return (
      <>
        <EmptyState title="Aucune estimation">
          Enregistrez la valeur du bien à une date (saisie, prix au m² ou estimation externe) pour suivre son évolution.
        </EmptyState>
        <div className="mt-3">{addButton}</div>
        {form}
      </>
    )
  }

  const first = points[0]
  const last = points[points.length - 1]
  const summary =
    first && last
      ? `Valeur du bien du ${date(first.date)} au ${date(last.date)} : de ${money(first.value)} à ${money(last.value)} ; valeur nette de dette aujourd’hui ${money(last.net)}.`
      : 'Évolution de la valeur du bien.'

  return (
    <div className="flex flex-col gap-3">
      {points.length > 1 ? (
        <figure>
          <div role="img" aria-label={summary} className="h-56 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={points} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
                <CartesianGrid stroke="var(--line)" vertical={false} />
                <XAxis
                  dataKey="date"
                  tickFormatter={(value: string) => date(value, 'month')}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={{ stroke: 'var(--line)' }}
                  minTickGap={32}
                />
                <YAxis tickFormatter={(value: number) => moneyCompact(value)} tick={AXIS_TICK} tickLine={false} axisLine={false} width={62} />
                <Tooltip content={(props) => <MoneyTooltip {...props} formatLabel={(value) => date(String(value))} />} />
                {SERIES.map((s) => (
                  <Line
                    key={s.key}
                    type="stepAfter"
                    dataKey={s.key}
                    name={s.label}
                    stroke={s.color}
                    strokeWidth={s.key === 'net' ? 2.5 : 1.5}
                    dot={false}
                    isAnimationActive={false}
                  />
                ))}
              </LineChart>
            </ResponsiveContainer>
          </div>
          <figcaption className="sr-only">{summary}</figcaption>
          <Legend items={SERIES.map((s) => ({ key: s.key, label: s.label, color: s.color }))} />
          <details className="mt-2 rounded-[10px] border border-line">
            <summary className="flex min-h-10 items-center px-3 text-[13.5px] font-medium" aria-controls={tableId}>
              Voir la courbe en tableau
            </summary>
            <div id={tableId} className="table-wrap max-h-[300px] overflow-y-auto border-t border-line">
              <table className="table">
                <caption className="sr-only">Valeur, capital restant dû et valeur nette du bien par date</caption>
                <thead>
                  <tr>
                    <th scope="col">Date</th>
                    <th scope="col" className="r">Valeur estimée</th>
                    <th scope="col" className="r">Restant dû</th>
                    <th scope="col" className="r">Valeur nette</th>
                  </tr>
                </thead>
                <tbody>
                  {points.map((point) => (
                    <tr key={point.date}>
                      <td>{date(point.date)}</td>
                      <td className="r">{money(point.value)}</td>
                      <td className="r">{money(point.debt)}</td>
                      <td className="r">{money(point.net)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
        </figure>
      ) : null}

      <div className="table-wrap">
        <table className="table">
          <caption className="sr-only">Estimations de {item.account.name}, de la plus récente à la plus ancienne</caption>
          <thead>
            <tr>
              <th scope="col">Date</th>
              <th scope="col" className="r">Valeur</th>
              <th scope="col">Source</th>
              <th scope="col">Note</th>
              <th scope="col">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {list.map((valuation, index) => (
              <tr key={valuation.id}>
                <td>
                  {date(valuation.valued_on)}
                  {index === 0 ? <span className="ml-1.5 text-[12.5px] text-muted">(en vigueur)</span> : null}
                </td>
                <td className="r">{money(valuation.value)}</td>
                <td>
                  {SOURCE_LABEL[valuation.source]}
                  {valuation.price_per_m2 !== null ? (
                    <span className="num text-muted"> · {money(valuation.price_per_m2)}/m²</span>
                  ) : null}
                </td>
                <td>{valuation.note ?? '–'}</td>
                <td className="r">
                  <IconButton
                    icon="trash"
                    label={`Supprimer l’estimation du ${date(valuation.valued_on)}`}
                    disabled={list.length <= 1}
                    onClick={() => {
                      remove.reset()
                      setDeleting(valuation)
                    }}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {list.length <= 1 ? (
        <p className="text-[13px] text-muted">La dernière estimation d’un bien ne peut pas être supprimée.</p>
      ) : null}
      <div>{addButton}</div>
      {form}

      {deleting ? (
        <ConfirmModal
          title="Supprimer cette estimation ?"
          confirmLabel="Supprimer l’estimation"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          L’estimation du {date(deleting.valued_on)} ({money(deleting.value)}) sera supprimée ; la valeur du bien suivra
          l’estimation la plus récente restante.
        </ConfirmModal>
      ) : null}
    </div>
  )
}
