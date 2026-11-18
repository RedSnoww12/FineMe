import { useMemo } from 'react'
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import type { DailyHistoryPoint } from '@/shared/lib/history'
import { countEstimatedDays } from '@/shared/lib/history'
import { date } from '@/shared/lib/format'
import { ACCOUNT_KIND_PLURAL, CATEGORY_COLOR } from '@/shared/lib/labels'
import { Legend, type TooltipProps } from '@/shared/ui/charts'
import { Panel, Signed } from '@/shared/ui/ui'
import { balanceRows, categoryRows, countUndetailedDays, presentCategories, type BalanceRow } from '../lib/evolution'
import { formatAxis, formatUnit, sampleForTable, type Unit } from '../lib/period'
import { periodVariation, waterfallSteps, type WaterfallStep, type WaterfallStepKey } from '../lib/waterfall'
import { AXIS_TICK, ChartFigure, ChartIntro, DataTable, dateTick, PanelState, UnitTooltip } from './chartParts'

export type HistoryPanelProps = {
  /** Série quotidienne de la période, jours manquants estimés. */
  series: DailyHistoryPoint[]
  unit: Unit
  isPending: boolean
  error: unknown
  onRetry: () => void
}

const TOO_SHORT =
  'L’historique se construit au fil du temps : FineMe enregistre chaque jour un point de votre patrimoine. Ce graphique apparaîtra dès le deuxième jour de la période.'

function EstimatedNote({ series }: { series: DailyHistoryPoint[] }) {
  const estimated = countEstimatedDays(series)
  if (estimated === 0) return null
  return (
    <p className="mt-2 text-[13px] text-muted">
      {estimated === 1 ? '1 jour' : `${estimated} jours`} sans instantané sur la période : valeurs estimées entre les
      relevés qui les encadrent.
    </p>
  )
}

function periodLabel(series: DailyHistoryPoint[]): string {
  return `du ${date(series[0]?.date, 'long')} au ${date(series[series.length - 1]?.date, 'long')}`
}

/** Aires empilées : la valeur de chaque catégorie d'actifs, jour après jour. */
export function CategoryEvolutionPanel({ series, unit, isPending, error, onRetry }: HistoryPanelProps) {
  const rows = useMemo(() => categoryRows(series, unit), [series, unit])
  const categories = presentCategories(rows)
  const undetailed = countUndetailedDays(series)
  const tick = dateTick(series[0]?.date, series[series.length - 1]?.date)
  const legend = categories.map((category) => ({ key: category, label: ACCOUNT_KIND_PLURAL[category], color: CATEGORY_COLOR[category] }))
  const last = [...rows].reverse().find((row) => row.checking !== null)

  return (
    <Panel title="Évolution des actifs par catégorie">
      <ChartIntro>
        {unit === 'eur'
          ? 'Chaque bande montre ce que vaut une catégorie de placements ; leur empilement donne le total de vos actifs, hors dettes.'
          : 'Chaque bande montre la part d’une catégorie dans vos actifs du jour ; l’empilement fait toujours 100 %.'}
      </ChartIntro>
      <PanelState
        isPending={isPending}
        error={error}
        onRetry={onRetry}
        empty={
          series.length < 2
            ? TOO_SHORT
            : categories.length === 0
              ? 'Le détail par catégorie est enregistré avec chaque nouvel instantané : il apparaîtra ici au fil des prochains jours.'
              : null
        }
      >
        {() => (
          <>
            <ChartFigure
              summary={`Actifs par catégorie ${periodLabel(series)}. Dernier relevé : ${legend
                .map((s) => `${s.label} ${formatUnit(last?.[s.key] ?? 0, unit)}`)
                .join(', ')}.`}
            >
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
                  <CartesianGrid stroke="var(--line)" vertical={false} />
                  <XAxis dataKey="date" tickFormatter={tick} tick={AXIS_TICK} tickLine={false} axisLine={{ stroke: 'var(--line)' }} minTickGap={40} />
                  <YAxis
                    tickFormatter={(value: number) => formatAxis(value, unit)}
                    tick={AXIS_TICK}
                    tickLine={false}
                    axisLine={false}
                    width={62}
                    domain={unit === 'pct' ? [0, 100] : [0, 'auto']}
                  />
                  <Tooltip content={(props) => <UnitTooltip {...props} unit={unit} series={[...legend].reverse()} />} />
                  {categories.map((category) => (
                    <Area
                      key={category}
                      type="linear"
                      dataKey={category}
                      name={ACCOUNT_KIND_PLURAL[category]}
                      stackId="actifs"
                      stroke={CATEGORY_COLOR[category]}
                      fill={CATEGORY_COLOR[category]}
                      fillOpacity={0.55}
                      isAnimationActive={false}
                    />
                  ))}
                </AreaChart>
              </ResponsiveContainer>
            </ChartFigure>
            <Legend items={legend} />
            <EstimatedNote series={series} />
            {undetailed > 0 ? (
              <p className="mt-1 text-[13px] text-muted">
                {undetailed === 1 ? '1 jour' : `${undetailed} jours`} de la période précèdent l’enregistrement du détail par
                catégorie et restent vides.
              </p>
            ) : null}
            <DataTable
              caption="Valeur des actifs par catégorie"
              columns={[
                { header: 'Date' },
                ...legend.map((s) => ({ header: s.label, numeric: true })),
              ]}
              rows={sampleForTable(rows.filter((row) => row.checking !== null)).map((row) => [
                <>
                  {date(row.date)}
                  {row.estimated ? <span className="text-muted"> (estimé)</span> : null}
                </>,
                ...legend.map((s) => formatUnit(row[s.key], unit)),
              ])}
            />
          </>
        )}
      </PanelState>
    </Panel>
  )
}

const BALANCE_SERIES: { key: 'gross_assets' | 'liabilities' | 'net_worth'; label: string; color: string }[] = [
  { key: 'gross_assets', label: 'Actifs bruts', color: 'var(--c5)' },
  { key: 'liabilities', label: 'Dettes', color: 'var(--neg)' },
  { key: 'net_worth', label: 'Patrimoine net', color: 'var(--c1)' },
]

/** Courbes : actifs bruts, dettes et patrimoine net (actifs moins dettes). */
export function BalancePanel({ series, unit, isPending, error, onRetry }: HistoryPanelProps) {
  const rows = useMemo(() => balanceRows(series, unit), [series, unit])
  const tick = dateTick(series[0]?.date, series[series.length - 1]?.date)
  const last: BalanceRow | undefined = rows[rows.length - 1]

  return (
    <Panel title="Actifs, dettes et patrimoine net">
      <ChartIntro>
        {unit === 'eur'
          ? 'Ce que vous possédez (actifs), ce que vous devez (dettes) et la différence : votre patrimoine net.'
          : 'Variation de chaque courbe depuis le premier jour de la période, en pourcentage.'}
      </ChartIntro>
      <PanelState isPending={isPending} error={error} onRetry={onRetry} empty={series.length < 2 ? TOO_SHORT : null}>
        {() => (
          <>
            <ChartFigure
              summary={`Actifs, dettes et patrimoine net ${periodLabel(series)}. Dernier relevé : ${BALANCE_SERIES
                .map((s) => `${s.label} ${formatUnit(last?.[s.key], unit, unit === 'pct')}`)
                .join(', ')}.`}
            >
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
                  <CartesianGrid stroke="var(--line)" vertical={false} />
                  <XAxis dataKey="date" tickFormatter={tick} tick={AXIS_TICK} tickLine={false} axisLine={{ stroke: 'var(--line)' }} minTickGap={40} />
                  <YAxis
                    tickFormatter={(value: number) => formatAxis(value, unit)}
                    tick={AXIS_TICK}
                    tickLine={false}
                    axisLine={false}
                    width={62}
                    domain={['auto', 'auto']}
                  />
                  {unit === 'pct' ? <ReferenceLine y={0} stroke="var(--muted)" strokeDasharray="3 3" /> : null}
                  <Tooltip content={(props) => <UnitTooltip {...props} unit={unit} series={BALANCE_SERIES} />} />
                  {BALANCE_SERIES.map((s) => (
                    <Line
                      key={s.key}
                      type="monotone"
                      dataKey={s.key}
                      name={s.label}
                      stroke={s.color}
                      strokeWidth={s.key === 'net_worth' ? 2.5 : 1.75}
                      dot={false}
                      connectNulls={false}
                      isAnimationActive={false}
                    />
                  ))}
                </LineChart>
              </ResponsiveContainer>
            </ChartFigure>
            <Legend items={BALANCE_SERIES} />
            <EstimatedNote series={series} />
            <DataTable
              caption="Actifs bruts, dettes et patrimoine net"
              columns={[{ header: 'Date' }, ...BALANCE_SERIES.map((s) => ({ header: s.label, numeric: true }))]}
              rows={sampleForTable(rows).map((row) => [
                <>
                  {date(row.date)}
                  {row.estimated ? <span className="text-muted"> (estimé)</span> : null}
                </>,
                ...BALANCE_SERIES.map((s) => formatUnit(row[s.key], unit, unit === 'pct')),
              ])}
            />
          </>
        )}
      </PanelState>
    </Panel>
  )
}

const STEP_SHORT: Record<WaterfallStepKey, string> = {
  start: 'Début de période',
  contributions: 'Apports',
  performance: 'Performance',
  debt_repayment: 'Dettes remboursées',
  end: 'Fin de période',
}

function stepColor(step: WaterfallStep): string {
  if (step.kind === 'total') return 'var(--c1)'
  return step.kind === 'up' ? 'var(--pos)' : 'var(--neg)'
}

function StepTooltip({ active, label, steps, unit }: TooltipProps & { steps: WaterfallStep[]; unit: Unit }) {
  const step = active ? steps.find((candidate) => candidate.key === label) : undefined
  if (!step) return null
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      <div className="font-medium">{step.label}</div>
      <div className="num">{formatUnit(step.amount, unit, step.kind !== 'total')}</div>
    </div>
  )
}

/** Cascade : du patrimoine net de début de période à celui de fin, par grandes causes de variation. */
export function WaterfallPanel({ series, unit, isPending, error, onRetry }: HistoryPanelProps) {
  const first = series[0]
  const last = series[series.length - 1]
  const variation = useMemo(() => (first && last && series.length > 1 ? periodVariation(first, last) : null), [first, last, series.length])
  const steps = useMemo(() => (variation ? waterfallSteps(variation, unit) : null), [variation, unit])
  const hasNegative = steps?.some((step) => step.range[0] < 0) ?? false

  return (
    <Panel title="Variation sur la période">
      <ChartIntro>
        D’où vient l’écart de patrimoine net entre le début et la fin de la période : vos apports (versements, retraits,
        revalorisations), la performance de vos placements et le capital de dettes remboursé.
        {unit === 'pct' ? ' En pourcentage du patrimoine net de début de période.' : ''}
      </ChartIntro>
      <PanelState
        isPending={isPending}
        error={error}
        onRetry={onRetry}
        empty={
          !variation
            ? TOO_SHORT
            : !steps
              ? 'Le patrimoine net était nul au début de la période : la variation ne peut pas s’exprimer en pourcentage. Passez en euros.'
              : null
        }
      >
        {() =>
          steps && variation ? (
            <>
              <ChartFigure
                summary={`Variation du patrimoine net ${periodLabel(series)} : ${steps
                  .map((step) => `${step.label} ${formatUnit(step.amount, unit, step.kind !== 'total')}`)
                  .join(', ')}.`}
              >
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={steps} layout="vertical" margin={{ top: 4, right: 8, bottom: 0, left: 0 }} accessibilityLayer={false}>
                    <CartesianGrid stroke="var(--line)" horizontal={false} />
                    <XAxis
                      type="number"
                      tickFormatter={(value: number) => formatAxis(value, unit)}
                      tick={AXIS_TICK}
                      tickLine={false}
                      axisLine={{ stroke: 'var(--line)' }}
                    />
                    <YAxis
                      type="category"
                      dataKey="key"
                      tickFormatter={(key: WaterfallStepKey) => STEP_SHORT[key]}
                      tick={AXIS_TICK}
                      tickLine={false}
                      axisLine={false}
                      width={112}
                      interval={0}
                    />
                    {hasNegative ? <ReferenceLine x={0} stroke="var(--muted)" /> : null}
                    <Tooltip cursor={{ fill: 'var(--surface-2)' }} content={(props) => <StepTooltip {...props} steps={steps} unit={unit} />} />
                    <Bar dataKey="range" radius={[4, 4, 4, 4]} isAnimationActive={false} minPointSize={2}>
                      {steps.map((step) => (
                        <Cell key={step.key} fill={stepColor(step)} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </ChartFigure>
              <Legend
                items={[
                  { label: 'Patrimoine net', color: 'var(--c1)' },
                  { label: 'Hausse', color: 'var(--pos)' },
                  { label: 'Baisse', color: 'var(--neg)' },
                ]}
              />
              {variation.performance === null ? (
                <p className="mt-2 text-[13px] text-muted">
                  La performance n’est pas isolée : le début de la période précède l’enregistrement du détail par compte.
                  Elle est comprise dans les apports.
                </p>
              ) : null}
              <DataTable
                caption="Décomposition de la variation du patrimoine net"
                columns={[{ header: 'Étape' }, { header: unit === 'eur' ? 'Montant' : 'Part du patrimoine de début', numeric: true }]}
                rows={steps.map((step) => [
                  step.label,
                  step.kind === 'total' ? (
                    formatUnit(step.amount, unit)
                  ) : (
                    <Signed key="montant" value={step.amount}>
                      {formatUnit(step.amount, unit, true)}
                    </Signed>
                  ),
                ])}
              />
            </>
          ) : null
        }
      </PanelState>
    </Panel>
  )
}

