import { useMemo, useState } from 'react'
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import type { Account, Goal } from '@/shared/api/types'
import { date, money, todayIso } from '@/shared/lib/format'
import type { DailyHistoryPoint } from '@/shared/lib/history'
import { Legend } from '@/shared/ui/charts'
import { Panel, SelectField, Stat } from '@/shared/ui/ui'
import { useLoanSchedules, useRealEstate, useValuationsOf } from '../hooks/useAnalysisApi'
import { gapToPlan, goalChartRows, goalPlannedSeries, goalRealSeries } from '../lib/goals'
import { formatAxis, formatUnit, type Unit } from '../lib/period'
import { viewFraction } from '../lib/performance'
import { equitySeries, yearlyRows, type LoanTimeline } from '../lib/realEstate'
import { AXIS_TICK, ChartFigure, ChartIntro, DataTable, PanelState, UnitTooltip, type AccountsState } from './chartParts'

const EQUITY_SERIES = [
  { key: 'repaid', label: 'Capital remboursé', color: 'var(--c3)' },
  { key: 'remaining', label: 'Capital restant dû', color: 'var(--c6)' },
  { key: 'equity', label: 'Valeur nette', color: 'var(--c1)' },
  { key: 'value', label: 'Valeur des biens', color: 'var(--c2)' },
] as const

/** Immobilier : capital remboursé et restant dû des prêts liés, et valeur nette des biens, sur la durée des prêts. */
export function RealEstatePanel({
  accounts,
  memberId,
  unit,
  isPending,
  error,
  onRetry,
}: { accounts: Account[]; memberId: string | null; unit: Unit } & AccountsState) {
  const realEstate = useRealEstate()
  const items = useMemo(() => realEstate.data?.properties ?? [], [realEstate.data])
  const loans = useMemo(() => items.flatMap((item) => item.loans), [items])
  const schedules = useLoanSchedules(loans.map((loan) => loan.account_id))
  const valuations = useValuationsOf(items.map((item) => item.account.id))
  const rows = useMemo(() => {
    if (!schedules.data || !valuations.data) return []
    const byProperty = new Map(items.map((item, i) => [item.account.id, valuations.data?.[i] ?? []]))
    const timelines: LoanTimeline[] = loans.map((loan, i) => {
      const owners = accounts.find((account) => account.id === loan.account_id)?.owners ?? []
      return { loan, fraction: viewFraction(owners, memberId), rows: schedules.data?.[i]?.rows ?? [] }
    })
    return equitySeries(items, timelines, unit, byProperty)
  }, [schedules.data, valuations.data, loans, accounts, memberId, items, unit])
  const thisMonth = `${todayIso().slice(0, 7)}-01`
  const current = rows.find((row) => row.date === thisMonth)
  const legend = EQUITY_SERIES.map((s) => ({ key: s.key, label: s.label, color: s.color }))

  return (
    <Panel title="Immobilier et prêts">
      <ChartIntro>
        Au fil des échéances, le capital remboursé grandit et le restant dû diminue : la valeur nette de vos biens (valeur
        moins restant dû) augmente. La valeur des biens suit leurs estimations datées, puis reste à la dernière.
        {unit === 'pct' ? ' En pourcentage de la valeur des biens.' : ''}
      </ChartIntro>
      <PanelState
        isPending={isPending || realEstate.isPending || (loans.length > 0 && schedules.isPending) || valuations.isPending}
        error={error ?? realEstate.error ?? schedules.error ?? valuations.error}
        onRetry={() => {
          onRetry()
          void realEstate.refetch()
          schedules.refetch()
          valuations.refetch()
        }}
        empty={
          items.length === 0
            ? 'Aucun bien immobilier dans cette vue. Ajoutez-en un depuis l’écran Immobilier.'
            : rows.length === 0
              ? `Aucun prêt lié à vos biens dans cette vue : leur valeur nette est leur valeur, ${money(realEstate.data?.totals.value)}.`
              : null
        }
      >
        {() => (
          <>
            {current ? (
              <div className="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
                <Stat label="Remboursé" value={formatUnit(current.repaid, unit)} />
                <Stat label="Restant dû" value={formatUnit(current.remaining, unit)} />
                <Stat label="Valeur nette" value={formatUnit(current.equity, unit)} />
              </div>
            ) : null}
            <ChartFigure
              summary={`Prêts immobiliers du ${date(rows[0]?.date, 'month')} au ${date(rows[rows.length - 1]?.date, 'month')}.${
                current
                  ? ` Ce mois-ci : ${formatUnit(current.repaid, unit)} remboursés, ${formatUnit(current.remaining, unit)} restant dus, valeur nette ${formatUnit(current.equity, unit)}.`
                  : ''
              }`}
            >
              <ResponsiveContainer width="100%" height="100%">
                <ComposedChart data={rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
                  <CartesianGrid stroke="var(--line)" vertical={false} />
                  <XAxis
                    dataKey="date"
                    tickFormatter={(value: string) => value.slice(0, 4)}
                    tick={AXIS_TICK}
                    tickLine={false}
                    axisLine={{ stroke: 'var(--line)' }}
                    minTickGap={32}
                  />
                  <YAxis
                    tickFormatter={(value: number) => formatAxis(value, unit)}
                    tick={AXIS_TICK}
                    tickLine={false}
                    axisLine={false}
                    width={62}
                  />
                  <Tooltip
                    content={(props) => (
                      <UnitTooltip {...props} unit={unit} series={legend} formatLabel={(value) => date(value, 'month')} />
                    )}
                  />
                  {current ? (
                    <ReferenceLine
                      x={thisMonth}
                      stroke="var(--muted)"
                      strokeDasharray="3 3"
                      label={{ value: 'Aujourd’hui', position: 'insideTopLeft', fill: 'var(--muted)', fontSize: 12 }}
                    />
                  ) : null}
                  <Area type="linear" dataKey="repaid" name="Capital remboursé" stackId="pret" stroke="var(--c3)" fill="var(--c3)" fillOpacity={0.45} isAnimationActive={false} />
                  <Area type="linear" dataKey="remaining" name="Capital restant dû" stackId="pret" stroke="var(--c6)" fill="var(--c6)" fillOpacity={0.35} isAnimationActive={false} />
                  <Line type="linear" dataKey="equity" name="Valeur nette" stroke="var(--c1)" strokeWidth={2.5} dot={false} isAnimationActive={false} />
                  <Line type="linear" dataKey="value" name="Valeur des biens" stroke="var(--c2)" strokeWidth={1.5} strokeDasharray="5 4" dot={false} isAnimationActive={false} />
                </ComposedChart>
              </ResponsiveContainer>
            </ChartFigure>
            <Legend items={legend} />
            <DataTable
              caption="Capital remboursé, restant dû et valeur nette, une ligne par an"
              columns={[{ header: 'Mois' }, ...legend.map((s) => ({ header: s.label, numeric: true }))]}
              rows={yearlyRows(rows).map((row) => [date(row.date, 'month'), ...legend.map((s) => formatUnit(row[s.key], unit))])}
            />
          </>
        )}
      </PanelState>
    </Panel>
  )
}

/** Objectifs visibles dans la vue : tous pour le foyer, ceux du membre (ou sans membre désigné) sinon. */
function goalsInView(goals: Goal[], memberId: string | null): Goal[] {
  if (memberId === null) return goals
  return goals.filter((goal) => goal.member_ids.length === 0 || goal.member_ids.includes(memberId))
}

const GOAL_SERIES = [
  { key: 'real', label: 'Montant atteint', color: 'var(--c1)' },
  { key: 'planned', label: 'Trajectoire prévue', color: 'var(--c2)' },
]

/** Objectifs : montant réellement atteint face à la trajectoire prévue par les versements et le rendement attendus. */
export function GoalsPanel({
  goals,
  memberId,
  householdSeries,
  unit,
  isPending,
  error,
  onRetry,
}: {
  goals: Goal[]
  memberId: string | null
  householdSeries: DailyHistoryPoint[]
  unit: Unit
  isPending: boolean
  error: unknown
  onRetry: () => void
}) {
  const visible = useMemo(() => goalsInView(goals, memberId), [goals, memberId])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const goal = visible.find((candidate) => candidate.id === selectedId) ?? visible[0]
  const chart = useMemo(() => {
    if (!goal) return null
    const real = goalRealSeries(goal, householdSeries)
    const start = real[0] ?? { date: todayIso(), amount: goal.progress.current_amount }
    const planned = goalPlannedSeries(goal, start)
    const target = goal.progress.target_amount
    const rows = goalChartRows(real, planned, target, unit).map((row) => ({ ...row, t: Date.parse(row.date) }))
    return { real, planned, rows, target, gap: gapToPlan(real, planned) }
  }, [goal, householdSeries, unit])

  return (
    <Panel title="Objectifs">
      <ChartIntro>
        Le montant réellement réuni sur les comptes liés à l’objectif, comparé à la trajectoire prévue par vos versements
        mensuels et le rendement attendu. Mesuré sur la part du foyer.
        {unit === 'pct' ? ' En pourcentage de la cible.' : ''}
      </ChartIntro>
      <PanelState
        isPending={isPending}
        error={error}
        onRetry={onRetry}
        empty={visible.length === 0 ? 'Aucun objectif pour cette vue. Créez-en un depuis l’écran Objectifs.' : null}
      >
        {() =>
          goal && chart ? (
            <>
              {visible.length > 1 ? (
                <SelectField
                  label="Objectif affiché"
                  value={goal.id}
                  onChange={setSelectedId}
                  options={visible.map((candidate) => ({ value: candidate.id, label: candidate.name }))}
                  fieldClassName="mb-3 max-w-sm"
                />
              ) : null}
              <div className="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
                <Stat label="Atteint" value={money(goal.progress.current_amount)} sub={`sur ${money(chart.target)}`} />
                <Stat label="Échéance" value={goal.target_date ? date(goal.target_date) : 'Aucune'} />
                {chart.gap !== null ? (
                  <Stat
                    label={chart.gap >= 0 ? 'Avance sur le plan' : 'Retard sur le plan'}
                    value={money(Math.abs(chart.gap))}
                    tone={chart.gap >= 0 ? 'pos' : 'neg'}
                  />
                ) : null}
              </div>
              {chart.rows.length < 2 ? (
                <p className="py-6 text-muted">
                  {goal.kind === 'project' && !goal.target_date
                    ? 'Fixez une échéance à ce projet pour tracer sa trajectoire prévue ; le montant atteint s’affichera au fil des jours.'
                    : 'Le montant atteint se trace à partir de deux jours d’historique détaillé sur la période.'}
                </p>
              ) : (
                <>
                  <ChartFigure
                    summary={`${goal.name} : ${money(goal.progress.current_amount)} atteints sur ${money(chart.target)}${
                      chart.gap !== null ? `, ${chart.gap >= 0 ? 'en avance' : 'en retard'} de ${money(Math.abs(chart.gap))} sur la trajectoire prévue` : ''
                    }.`}
                  >
                    <ResponsiveContainer width="100%" height="100%">
                      <LineChart data={chart.rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
                        <CartesianGrid stroke="var(--line)" vertical={false} />
                        <XAxis
                          dataKey="t"
                          type="number"
                          scale="time"
                          domain={['dataMin', 'dataMax']}
                          tickFormatter={(t: number) => date(new Date(t).toISOString().slice(0, 10), 'month')}
                          tick={AXIS_TICK}
                          tickLine={false}
                          axisLine={{ stroke: 'var(--line)' }}
                          minTickGap={40}
                        />
                        <YAxis
                          tickFormatter={(value: number) => formatAxis(value, unit)}
                          tick={AXIS_TICK}
                          tickLine={false}
                          axisLine={false}
                          width={62}
                          domain={[0, 'auto']}
                        />
                        <ReferenceLine
                          y={unit === 'eur' ? chart.target : 100}
                          stroke="var(--muted)"
                          strokeDasharray="3 3"
                          label={{ value: 'Cible', position: 'insideTopRight', fill: 'var(--muted)', fontSize: 12 }}
                        />
                        <Tooltip
                          content={(props) => (
                            <UnitTooltip
                              {...props}
                              unit={unit}
                              series={GOAL_SERIES}
                              formatLabel={(t) => date(new Date(Number(t)).toISOString().slice(0, 10), 'long')}
                            />
                          )}
                        />
                        <Line type="linear" dataKey="planned" name="Trajectoire prévue" stroke="var(--c2)" strokeWidth={1.75} strokeDasharray="5 4" dot={false} connectNulls isAnimationActive={false} />
                        <Line type="linear" dataKey="real" name="Montant atteint" stroke="var(--c1)" strokeWidth={2.5} dot={false} connectNulls isAnimationActive={false} />
                      </LineChart>
                    </ResponsiveContainer>
                  </ChartFigure>
                  <Legend items={GOAL_SERIES} />
                </>
              )}
              {chart.planned.length === 0 && goal.kind === 'safety_net' ? (
                <p className="mt-2 text-[13px] text-muted">Une épargne de précaution n’a pas d’échéance : seule la cible est tracée.</p>
              ) : null}
              <DataTable
                caption={`Trajectoire de l’objectif ${goal.name}`}
                columns={[
                  { header: 'Date' },
                  { header: 'Montant atteint', numeric: true },
                  { header: 'Trajectoire prévue', numeric: true },
                ]}
                rows={chart.rows
                  .filter((row, i, all) => row.planned !== null || i === all.length - 1 || all[i + 1]?.date.slice(0, 7) !== row.date.slice(0, 7))
                  .map((row) => [date(row.date), formatUnit(row.real, unit), formatUnit(row.planned, unit)])}
              />
            </>
          ) : null
        }
      </PanelState>
    </Panel>
  )
}
