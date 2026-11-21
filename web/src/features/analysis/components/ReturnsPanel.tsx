import { useMemo, useState, type FormEvent } from 'react'
import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { Performance, PerformanceReport } from '@/shared/api/types'
import { date, money, number, pctFromFraction, todayIso } from '@/shared/lib/format'
import { Legend, type TooltipProps } from '@/shared/ui/charts'
import { Button, Dot, FormError, Notice, Panel, SelectField, Signed, TextField } from '@/shared/ui/ui'
import { useBenchmarkMutations, usePerformance } from '../hooks/useAnalysisApi'
import type { Period } from '../lib/period'
import {
  benchmarkInput,
  CURVE_EXPLANATION,
  curveRows,
  EMPTY_BENCHMARK_FORM,
  formatMeasure,
  isPartialYear,
  MEASURES,
  performanceBounds,
  undefinedReason,
  YEARLY_EXPLANATION,
  yearsNewestFirst,
  type BenchmarkForm,
} from '../lib/returns'
import { AXIS_TICK, ChartFigure, ChartIntro, DataTable, dateTick, PanelState } from './chartParts'

const PORTFOLIO = 'portfolio'
const NO_HISTORY =
  'Pas encore assez d’historique pour vos comptes de titres et de crypto sur cette période : la performance se mesure entre deux relevés quotidiens au moins.'

const CURVE_SERIES = [
  { key: 'portfolio', label: 'Vos placements', color: 'var(--c1)' },
  { key: 'benchmark', label: 'Indice', color: 'var(--c3)' },
] as const

/** Mesure colorée selon son signe ; « Indéfini » plutôt qu'un tiret quand elle manque. */
function MeasureValue({ value }: { value: number | null }) {
  if (value === null) return <span className="text-muted">{formatMeasure(value)}</span>
  return <Signed value={value}>{formatMeasure(value)}</Signed>
}

function Measures({ performance }: { performance: Performance }) {
  return (
    <dl className="grid gap-x-5 gap-y-4 sm:grid-cols-2">
      {MEASURES.map((measure) => {
        const value = performance[measure.key]
        return (
          <div key={measure.key} className="min-w-0">
            <dt className="text-[13.5px] text-muted">{measure.label}</dt>
            <dd className="num mt-0.5 font-display text-[21px] font-semibold">
              <MeasureValue value={value} />
            </dd>
            <dd className="mt-0.5 text-[13px] text-muted">{measure.explanation}</dd>
            {value === null ? <dd className="mt-0.5 text-[13px] text-muted">{undefinedReason(measure.key)}</dd> : null}
          </div>
        )
      })}
    </dl>
  )
}

function CurveTooltip({ active, payload, label }: TooltipProps) {
  if (!active || !payload || payload.length === 0) return null
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      <div className="mb-1 font-medium">{date(String(label ?? ''), 'long')}</div>
      <ul className="flex flex-col gap-0.5">
        {payload
          .filter((entry) => typeof entry.value === 'number')
          .map((entry, i) => (
            <li key={i} className="flex items-center justify-between gap-4">
              <span className="flex items-center gap-1.5 text-muted">
                {entry.color ? <Dot color={entry.color} /> : null}
                {String(entry.name ?? '')}
              </span>
              <span className="num font-medium">{number(Number(entry.value), 1)}</span>
            </li>
          ))}
      </ul>
    </div>
  )
}

function Curve({ performance, benchmarkName }: { performance: Performance; benchmarkName: string | null }) {
  const rows = useMemo(() => curveRows(performance), [performance])
  const hasBenchmark = rows.some((row) => row.benchmark !== null)
  const series = CURVE_SERIES.filter((s) => s.key === 'portfolio' || hasBenchmark).map((s) =>
    s.key === 'benchmark' && benchmarkName ? { ...s, label: benchmarkName } : s,
  )
  const tick = dateTick(rows[0]?.date, rows[rows.length - 1]?.date)
  const last = rows[rows.length - 1]
  if (rows.length < 2) return null
  return (
    <>
      <h3 className="mt-5 text-[14.5px] font-semibold">Évolution de 100 € placés</h3>
      <ChartIntro>{CURVE_EXPLANATION}</ChartIntro>
      <ChartFigure
        summary={`Base 100 du ${date(performance.from)} au ${date(performance.to)} : vos placements ${number(last?.portfolio, 1)}${
          hasBenchmark && last?.benchmark !== null ? `, indice ${number(last?.benchmark, 1)}` : ''
        }.`}
      >
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
            <CartesianGrid stroke="var(--line)" vertical={false} />
            <XAxis dataKey="date" tickFormatter={tick} tick={AXIS_TICK} tickLine={false} axisLine={{ stroke: 'var(--line)' }} minTickGap={40} />
            <YAxis tick={AXIS_TICK} tickLine={false} axisLine={false} width={48} domain={['auto', 'auto']} />
            <ReferenceLine y={100} stroke="var(--muted)" strokeDasharray="3 3" />
            <Tooltip content={(props) => <CurveTooltip {...props} />} />
            {series.map((s) => (
              <Line
                key={s.key}
                type="monotone"
                dataKey={s.key}
                name={s.label}
                stroke={s.color}
                strokeWidth={s.key === 'portfolio' ? 2.5 : 1.75}
                dot={false}
                connectNulls={false}
                isAnimationActive={false}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      </ChartFigure>
      <Legend items={series.map((s) => ({ key: s.key, label: s.label, color: s.color }))} />
      <DataTable
        caption="Évolution de 100 € placés"
        columns={[{ header: 'Date' }, ...series.map((s) => ({ header: s.label, numeric: true }))]}
        rows={rows.map((row) => [
          date(row.date),
          number(row.portfolio, 1),
          ...(hasBenchmark ? [row.benchmark === null ? '–' : number(row.benchmark, 1)] : []),
        ])}
      />
    </>
  )
}

function Years({ performance }: { performance: Performance }) {
  const years = yearsNewestFirst(performance)
  if (years.length === 0) return null
  return (
    <>
      <h3 className="mt-5 text-[14.5px] font-semibold">Année par année</h3>
      <ChartIntro>{YEARLY_EXPLANATION}</ChartIntro>
      <div className="table-wrap">
        <table className="table">
          <caption className="sr-only">Performance des placements par année civile</caption>
          <thead>
            <tr>
              <th scope="col">Année</th>
              <th scope="col">Période mesurée</th>
              <th scope="col" className="r">
                Performance
              </th>
            </tr>
          </thead>
          <tbody>
            {years.map((year) => (
              <tr key={year.year}>
                <td>
                  {year.year}
                  {isPartialYear(year) ? <span className="text-muted"> (partielle)</span> : null}
                </td>
                <td>
                  du {date(year.from)} au {date(year.to)}
                </td>
                <td className="r num">
                  <MeasureValue value={year.twr} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  )
}

function BenchmarkFormFields({ onDone, initial }: { onDone: () => void; initial: BenchmarkForm }) {
  const [form, setForm] = useState<BenchmarkForm>(initial)
  const [error, setError] = useState<string | null>(null)
  const { save } = useBenchmarkMutations()
  const set = (change: Partial<BenchmarkForm>) => setForm((current) => ({ ...current, ...change }))
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const input = benchmarkInput(form)
    if (typeof input === 'string') return setError(input)
    setError(null)
    save.mutate(input, { onSuccess: onDone })
  }
  return (
    <form onSubmit={submit} className="mt-3 flex flex-col gap-3" aria-label="Indice de référence">
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField label="Nom de l’indice" value={form.name} onChange={(name) => set({ name })} placeholder="ETF Monde" />
        <SelectField
          label="Type"
          value={form.kind}
          onChange={(kind) => set({ kind: kind === 'crypto' ? 'crypto' : 'security' })}
          options={[
            { value: 'security', label: 'Titre coté (ETF, action)' },
            { value: 'crypto', label: 'Crypto' },
          ]}
        />
        <TextField label="Ticker" value={form.symbol} onChange={(symbol) => set({ symbol })} hint="Ticker ou ISIN, au moins l’un des deux." />
        <TextField label="ISIN" value={form.isin} onChange={(isin) => set({ isin })} />
        <TextField label="Devise" value={form.currency} onChange={(currency) => set({ currency })} maxLength={3} />
      </div>
      <FormError error={error ?? save.error} />
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" busy={save.isPending}>
          Enregistrer l’indice
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Annuler
        </Button>
      </div>
    </form>
  )
}

function BenchmarkSection({ report, performance }: { report: PerformanceReport; performance: Performance | null }) {
  const [editing, setEditing] = useState(false)
  const { remove, save } = useBenchmarkMutations()
  const benchmark = report.benchmark
  const comparison = benchmark?.comparison ?? null
  return (
    <section aria-label="Comparaison à un indice" className="mt-5 border-t border-line pt-4">
      <h3 className="text-[14.5px] font-semibold">Comparaison à un indice</h3>
      <ChartIntro>
        Un indice de référence (par exemple un ETF Monde) montre ce qu’aurait donné un placement simple et diversifié sur la même
        période.
      </ChartIntro>
      {benchmark ? (
        <div className="flex flex-col gap-2 text-[14px]">
          <p>
            <span className="font-medium">{benchmark.name}</span>
            <span className="text-muted">
              {' '}
              ({[benchmark.symbol, benchmark.isin].filter(Boolean).join(' · ')}, {benchmark.currency}) :{' '}
              {benchmark.prices > 0 && benchmark.first_date && benchmark.last_date
                ? `${benchmark.prices} cours connus, du ${date(benchmark.first_date)} au ${date(benchmark.last_date)}.`
                : 'aucun cours connu pour l’instant.'}
            </span>
          </p>
          {comparison && performance ? (
            <p>
              Du {date(comparison.from)} au {date(comparison.to)}, l’indice a fait{' '}
              <Signed value={comparison.benchmark_return}>{pctFromFraction(comparison.benchmark_return, 1, true)}</Signed> et vos
              placements <Signed value={comparison.portfolio_return}>{pctFromFraction(comparison.portfolio_return, 1, true)}</Signed>.
            </p>
          ) : null}
          {benchmark.message ? <Notice>{benchmark.message}</Notice> : null}
          {save.data?.last_error ? <Notice>Cours du jour non relevé : {save.data.last_error}</Notice> : null}
          <FormError error={remove.error} />
          {editing ? null : (
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => setEditing(true)}>
                Changer d’indice
              </Button>
              <Button size="sm" variant="ghost" busy={remove.isPending} onClick={() => remove.mutate()}>
                Retirer l’indice
              </Button>
            </div>
          )}
        </div>
      ) : editing ? null : (
        <div className="flex flex-wrap items-center gap-3 text-[14px]">
          <span className="text-muted">Aucun indice de référence choisi.</span>
          <Button size="sm" onClick={() => setEditing(true)}>
            Choisir un indice
          </Button>
        </div>
      )}
      {editing ? (
        <BenchmarkFormFields
          onDone={() => setEditing(false)}
          initial={
            benchmark
              ? {
                  name: benchmark.name,
                  symbol: benchmark.symbol ?? '',
                  isin: benchmark.isin ?? '',
                  kind: benchmark.kind,
                  currency: benchmark.currency,
                }
              : EMPTY_BENCHMARK_FORM
          }
        />
      ) : null}
    </section>
  )
}

/** Rendement des comptes de titres et de crypto : TWR, MWR, années civiles et comparaison à un indice. */
export function ReturnsPanel({ period }: { period: Period }) {
  const bounds = useMemo(() => performanceBounds(period, todayIso()), [period])
  const query = usePerformance(bounds)
  const [scope, setScope] = useState(PORTFOLIO)
  const report = query.data
  const account = report?.accounts.find((a) => a.account_id === scope)
  const selected = account ? account.performance : (report?.portfolio ?? null)
  const options = [
    { value: PORTFOLIO, label: 'Tous les placements' },
    ...(report?.accounts ?? []).map((a) => ({ value: a.account_id, label: a.name })),
  ]

  return (
    <Panel
      title="Rendement des placements"
      action={
        report && report.accounts.length > 1 ? (
          <SelectField label="Mesurer" value={account ? scope : PORTFOLIO} onChange={setScope} options={options} fieldClassName="w-full sm:w-56" />
        ) : null
      }
    >
      <ChartIntro>
        Comptes de titres et de crypto, d’après l’historique quotidien et les versements ou retraits de leur journal d’opérations.
      </ChartIntro>
      <PanelState
        isPending={bounds !== null && query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        empty={bounds === null ? 'Choisissez une période valide.' : null}
      >
        {() =>
          report ? (
            <>
              {selected ? (
                <>
                  <p className="mb-4 text-[13.5px] text-muted">
                    Du {date(selected.from)} au {date(selected.to)} : {money(selected.start_value)} au départ,{' '}
                    {money(selected.net_flows, { signed: true })} versés ou retirés, {money(selected.end_value)} à la fin, soit{' '}
                    <Signed value={selected.gain}>{money(selected.gain, { signed: true })}</Signed> de gain.
                  </p>
                  <Measures performance={selected} />
                  <Curve performance={selected} benchmarkName={account ? null : (report.benchmark?.name ?? null)} />
                  <Years performance={selected} />
                </>
              ) : (
                <p className="py-6 text-muted">{NO_HISTORY}</p>
              )}
              <BenchmarkSection report={report} performance={account ? null : report.portfolio} />
            </>
          ) : null
        }
      </PanelState>
    </Panel>
  )
}
