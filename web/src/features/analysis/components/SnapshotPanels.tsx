import { useMemo, useState } from 'react'
import { Bar, BarChart, CartesianGrid, Cell, Pie, PieChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { Account, Overview } from '@/shared/api/types'
import { money, pct } from '@/shared/lib/format'
import { Dot, Panel, Segmented, Signed } from '@/shared/ui/ui'
import { useAccountDetails } from '../hooks/useAnalysisApi'
import { breakdownRows, DIMENSION_LABEL, toSlices, type Dimension, type Slice } from '../lib/breakdown'
import { formatAxis, formatUnit, type Unit } from '../lib/period'
import { accountPerformance, holdingPerformance, holdsPositions, type PerformanceRow } from '../lib/performance'
import { AXIS_TICK, ChartFigure, ChartIntro, DataTable, PanelState, type AccountsState } from './chartParts'

/** Anneau : répartition des actifs bruts d'aujourd'hui selon l'axe choisi. */
export function BreakdownPanel({ overview, unit, showMembers }: { overview: Overview; unit: Unit; showMembers: boolean }) {
  const [dimension, setDimension] = useState<Dimension>('category')
  const active: Dimension = !showMembers && dimension === 'member' ? 'category' : dimension
  const slices = useMemo(() => toSlices(breakdownRows(overview, active)), [overview, active])
  const options = (Object.keys(DIMENSION_LABEL) as Dimension[])
    .filter((key) => showMembers || key !== 'member')
    .map((key) => ({ value: key, label: DIMENSION_LABEL[key] }))
  const [focused, setFocused] = useState<Slice | null>(null)

  return (
    <Panel
      title="Répartition des actifs"
      action={
        <div className="max-w-full overflow-x-auto">
          <Segmented label="Répartir par" value={active} onChange={setDimension} options={options} />
        </div>
      }
    >
      <ChartIntro>
        Comment vos actifs d’aujourd’hui (hors dettes) se répartissent par {DIMENSION_LABEL[active].toLowerCase()}.
        {active === 'member' ? ' Chaque membre compte pour sa part de détention.' : ''}
      </ChartIntro>
      {slices.length === 0 ? (
        <p className="py-6 text-muted">
          {active === 'envelope'
            ? 'Aucune enveloppe fiscale renseignée sur vos comptes.'
            : 'Aucun actif à répartir pour cette vue.'}
        </p>
      ) : (
        <div className="flex flex-col items-center gap-5 sm:flex-row sm:items-start lg:flex-col lg:items-center 2xl:flex-row 2xl:items-start">
          <ChartFigure
            className="relative size-[176px] shrink-0"
            summary={`Répartition par ${DIMENSION_LABEL[active].toLowerCase()} : ${slices
              .map((slice) => `${slice.label} ${pct(slice.pct, 0)}`)
              .join(', ')}.`}
          >
            <ResponsiveContainer width="100%" height="100%">
              <PieChart accessibilityLayer={false}>
                <Pie
                  data={slices}
                  dataKey="value"
                  nameKey="label"
                  innerRadius={56}
                  outerRadius={86}
                  paddingAngle={slices.length > 1 ? 2 : 0}
                  stroke="none"
                  isAnimationActive={false}
                  onMouseEnter={(_, index) => setFocused(slices[index] ?? null)}
                  onMouseLeave={() => setFocused(null)}
                >
                  {slices.map((slice) => (
                    <Cell key={slice.key} fill={slice.color} />
                  ))}
                </Pie>
              </PieChart>
            </ResponsiveContainer>
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center px-8 text-center">
              <span className="line-clamp-2 text-[12px] leading-tight text-muted">{focused?.label ?? 'Actifs bruts'}</span>
              <span className="num text-[15px] font-semibold">
                {focused ? formatUnit(unit === 'eur' ? focused.value : focused.pct, unit) : money(overview.gross_assets)}
              </span>
            </div>
          </ChartFigure>
          <div className="table-wrap w-full min-w-0">
            <table className="table">
              <caption className="sr-only">Répartition des actifs par {DIMENSION_LABEL[active].toLowerCase()}</caption>
              <thead>
                <tr>
                  <th scope="col">{DIMENSION_LABEL[active]}</th>
                  <th scope="col" className="r">
                    Part
                  </th>
                  <th scope="col" className="r">
                    Valeur
                  </th>
                </tr>
              </thead>
              <tbody>
                {slices.map((slice) => (
                  <tr key={slice.key}>
                    <td>
                      <span className="flex min-w-0 items-center gap-2">
                        <Dot color={slice.color} />
                        <span className="truncate">{slice.label}</span>
                      </span>
                    </td>
                    <td className={unit === 'pct' ? 'r font-medium' : 'r text-muted'}>{pct(slice.pct, 1)}</td>
                    <td className={unit === 'eur' ? 'r font-medium' : 'r text-muted'}>{money(slice.value)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Panel>
  )
}

type PerformanceMode = 'accounts' | 'holdings'
const MAX_BARS = 12

function truncate(label: string, max = 18): string {
  return label.length > max ? `${label.slice(0, max - 1)}…` : label
}

/** Barres horizontales : plus-value latente par compte ou par position, depuis l'achat. */
export function PerformancePanel({
  accounts,
  memberId,
  unit,
  isPending,
  error,
  onRetry,
}: {
  accounts: Account[]
  memberId: string | null
  unit: Unit
} & AccountsState) {
  const [mode, setMode] = useState<PerformanceMode>('accounts')
  const positionAccounts = useMemo(
    () => (mode === 'holdings' ? accounts.filter(holdsPositions).map((account) => account.id) : []),
    [accounts, mode],
  )
  const details = useAccountDetails(positionAccounts)
  const rows: PerformanceRow[] = useMemo(() => {
    const all = mode === 'accounts' ? accountPerformance(accounts, memberId) : holdingPerformance(details.data ?? [], memberId)
    return unit === 'pct' ? all.filter((row) => row.pct !== null).sort((a, b) => (b.pct ?? 0) - (a.pct ?? 0)) : all
  }, [mode, accounts, memberId, details.data, unit])
  const charted = rows.slice(0, MAX_BARS).map((row) => ({ ...row, metric: unit === 'eur' ? row.gain : (row.pct ?? 0) }))
  const noun = mode === 'accounts' ? 'compte' : 'position'

  return (
    <Panel
      title="Performance"
      action={
        <Segmented
          label="Performance par"
          value={mode}
          onChange={setMode}
          options={[
            { value: 'accounts', label: 'Comptes' },
            { value: 'holdings', label: 'Positions' },
          ]}
        />
      }
    >
      <ChartIntro>
        Plus-value latente depuis l’achat, par {noun} :{' '}
        {unit === 'eur' ? 'le gain ou la perte en euros, à votre part.' : 'le gain ou la perte rapporté au montant investi.'} Seuls
        les {noun}s dont le prix d’achat est connu figurent ici.
      </ChartIntro>
      <PanelState
        isPending={isPending || (mode === 'holdings' && details.isPending && positionAccounts.length > 0)}
        error={error ?? (mode === 'holdings' ? details.error : null)}
        onRetry={() => {
          onRetry()
          details.refetch()
        }}
        empty={
          rows.length === 0
            ? mode === 'accounts'
              ? 'Renseignez le prix d’achat de vos positions et de vos biens pour mesurer leur performance.'
              : 'Aucune position avec un prix d’achat connu pour cette vue.'
            : null
        }
      >
        {() => (
          <>
            <ChartFigure
              className="w-full"
              summary={`Performance par ${noun} : ${charted
                .map((row) => `${row.label} ${formatUnit(row.metric, unit, true)}`)
                .join(', ')}.`}
            >
              <div style={{ height: Math.max(140, charted.length * 34 + 30) }}>
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={charted} layout="vertical" margin={{ top: 4, right: 8, bottom: 0, left: 0 }} accessibilityLayer={false}>
                    <CartesianGrid stroke="var(--line)" horizontal={false} />
                    <XAxis
                      type="number"
                      dataKey="metric"
                      tickFormatter={(value: number) => formatAxis(value, unit)}
                      tick={AXIS_TICK}
                      tickLine={false}
                      axisLine={{ stroke: 'var(--line)' }}
                    />
                    <YAxis
                      type="category"
                      dataKey="label"
                      tickFormatter={(label: string) => truncate(label)}
                      tick={AXIS_TICK}
                      tickLine={false}
                      axisLine={false}
                      width={128}
                    />
                    <ReferenceLine x={0} stroke="var(--muted)" />
                    <Tooltip
                      cursor={{ fill: 'var(--surface-2)' }}
                      content={({ active, label }) => {
                        const row = active ? charted.find((candidate) => candidate.label === label) : undefined
                        if (!row) return null
                        return (
                          <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
                            <div className="font-medium">{row.label}</div>
                            {row.detail ? <div className="text-muted">{row.detail}</div> : null}
                            <div className="num mt-1">
                              {money(row.gain, { signed: true })} · {pct(row.pct, 1, true)}
                            </div>
                          </div>
                        )
                      }}
                    />
                    <Bar dataKey="metric" radius={[3, 3, 3, 3]} isAnimationActive={false} minPointSize={2}>
                      {charted.map((row) => (
                        <Cell key={row.key} fill={row.metric < 0 ? 'var(--neg)' : 'var(--pos)'} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </ChartFigure>
            {rows.length > MAX_BARS ? (
              <p className="mt-2 text-[13px] text-muted">
                Les {MAX_BARS} premiers {noun}s sont tracés ; le tableau les liste tous ({rows.length}).
              </p>
            ) : null}
            <DataTable
              caption={`Performance par ${noun}`}
              columns={[
                { header: mode === 'accounts' ? 'Compte' : 'Position' },
                { header: 'Investi', numeric: true },
                { header: 'Plus-value', numeric: true },
                { header: 'Performance', numeric: true },
              ]}
              rows={rows.map((row) => [
                <>
                  <div className="font-medium">{row.label}</div>
                  {row.detail ? <div className="text-[13px] text-muted">{row.detail}</div> : null}
                </>,
                money(row.invested),
                <Signed key="gain" value={row.gain}>
                  {money(row.gain, { signed: true })}
                </Signed>,
                <Signed key="pct" value={row.pct}>
                  {pct(row.pct, 1, true)}
                </Signed>,
              ])}
            />
          </>
        )}
      </PanelState>
    </Panel>
  )
}
