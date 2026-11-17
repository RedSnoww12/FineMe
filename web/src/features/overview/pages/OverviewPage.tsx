import { useMemo, useState } from 'react'
import { Link } from 'react-router'
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { useMembers, useOverview } from '@/shared/api/queries'
import { useHistory } from '../hooks/useHistory'
import type { HistoryRange, Overview } from '@/shared/api/types'
import { AXIS_TICK, MoneyTooltip, type TooltipProps } from '@/shared/ui/charts'
import {
  Dot,
  EmptyState,
  ErrorState,
  LoadingBlock,
  PageHeader,
  Panel,
  Segmented,
  Signed,
  Skeleton,
  Stat,
} from '@/shared/ui/ui'
import { date, money, moneyCompact, pct, pctFromFraction } from '@/shared/lib/format'
import { countEstimatedDays, fillMissingDays, type HistoryChartRow, toHistoryChartRows } from '@/shared/lib/history'
import { CATEGORY_COLOR, envelopeLabel, SERIES_COLORS } from '@/shared/lib/labels'
import { useView } from '@/shared/state/view'

const RANGES: { value: HistoryRange; label: string }[] = [
  { value: '1m', label: '1 mois' },
  { value: '3m', label: '3 mois' },
  { value: '1y', label: '1 an' },
  { value: 'all', label: 'Tout' },
]

/** Infobulle de l'historique : la valeur du jour, en précisant si elle est estimée. */
function HistoryTooltip({ active, label, rows }: TooltipProps & { rows: ReadonlyMap<string, HistoryChartRow> }) {
  const row = active ? rows.get(String(label)) : undefined
  if (!row) return null
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      <div className="mb-1 font-medium">{date(row.date, 'long')}</div>
      <div className="flex items-center justify-between gap-4">
        <span className="text-muted">Patrimoine net</span>
        <span className="num font-medium">{money(row.net_worth)}</span>
      </div>
      {row.estimated ? <div className="mt-1 text-muted">Estimé : aucun instantané ce jour-là</div> : null}
    </div>
  )
}

function HistoryPanel() {
  const [range, setRange] = useState<HistoryRange>('3m')
  const history = useHistory(range)
  const points = history.data?.points
  const rows = useMemo(() => toHistoryChartRows(fillMissingDays(points ?? [])), [points])
  const rowsByDate = useMemo(() => new Map(rows.map((row) => [row.date, row])), [rows])
  const estimatedDays = countEstimatedDays(rows)
  const longRange = range === '1y' || range === 'all'

  return (
    <Panel
      title="Évolution du patrimoine net"
      action={<Segmented label="Période" value={range} onChange={setRange} options={RANGES} />}
    >
      {history.isPending ? (
        <Skeleton className="h-[240px]" />
      ) : history.isError ? (
        <ErrorState error={history.error} onRetry={() => void history.refetch()} />
      ) : rows.length < 2 ? (
        <p className="py-8 text-muted">
          L’historique se construit au fil du temps : FineMe enregistre chaque jour un point de votre patrimoine. La
          courbe apparaîtra dès le deuxième jour.
        </p>
      ) : (
        <>
          <div className="h-[240px] sm:h-[280px]">
            <ResponsiveContainer width="100%" height="100%">
              <ComposedChart data={rows} margin={{ top: 8, right: 4, bottom: 0, left: 0 }}>
                <defs>
                  <linearGradient id="net-fill" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="var(--c1)" stopOpacity={0.22} />
                    <stop offset="100%" stopColor="var(--c1)" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <CartesianGrid stroke="var(--line)" vertical={false} />
                <XAxis
                  dataKey="date"
                  tickFormatter={(value: string) => (longRange ? date(value, 'month') : date(value).slice(0, 5))}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={{ stroke: 'var(--line)' }}
                  minTickGap={40}
                />
                <YAxis
                  tickFormatter={(value: number) => moneyCompact(value)}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={false}
                  width={62}
                  domain={['auto', 'auto']}
                />
                <Tooltip content={(props) => <HistoryTooltip {...props} rows={rowsByDate} />} />
                <Area
                  type="monotone"
                  dataKey="net_worth"
                  name="Patrimoine net"
                  stroke="none"
                  fill="url(#net-fill)"
                  activeDot={false}
                  isAnimationActive={false}
                />
                <Line
                  type="monotone"
                  dataKey="measured"
                  name="Patrimoine net"
                  stroke="var(--c1)"
                  strokeWidth={2}
                  dot={false}
                  isAnimationActive={false}
                />
                <Line
                  type="monotone"
                  dataKey="estimate"
                  name="Patrimoine net estimé"
                  stroke="var(--c1)"
                  strokeWidth={2}
                  strokeDasharray="4 4"
                  strokeOpacity={0.7}
                  dot={false}
                  activeDot={false}
                  isAnimationActive={false}
                />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
          {estimatedDays > 0 ? (
            <p className="mt-2 text-[13px] text-muted">
              Pointillés : {estimatedDays === 1 ? '1 jour' : `${estimatedDays} jours`} sans instantané sur la période,
              valeurs estimées entre les relevés qui les encadrent.
            </p>
          ) : null}
        </>
      )}
    </Panel>
  )
}

/** Bande de strates : chaque catégorie occupe sa part des actifs bruts. */
function StrataBar({ allocation }: { allocation: Overview['allocation'] }) {
  const visible = allocation.filter((slice) => slice.value > 0)
  if (visible.length === 0) return null
  const total = visible.reduce((sum, slice) => sum + slice.value, 0)
  return (
    <div
      className="flex h-3.5 gap-[3px] overflow-hidden rounded-full"
      role="img"
      aria-label={`Répartition des actifs : ${visible.map((s) => `${s.label} ${pct(s.pct, 0)}`).join(', ')}`}
    >
      {visible.map((slice) => (
        <span
          key={slice.category}
          title={`${slice.label} : ${money(slice.value)}`}
          className="h-full min-w-[4px] rounded-[3px]"
          style={{ width: `${(slice.value / total) * 100}%`, background: CATEGORY_COLOR[slice.category] }}
        />
      ))}
    </div>
  )
}

function AllocationPanel({ allocation }: { allocation: Overview['allocation'] }) {
  const slices = allocation.filter((slice) => slice.value > 0).map((slice) => ({ ...slice, fill: CATEGORY_COLOR[slice.category] }))
  return (
    <Panel title="Répartition par catégorie">
      {slices.length === 0 ? (
        <p className="text-muted">Aucun actif à répartir pour cette vue.</p>
      ) : (
        <div className="flex flex-col items-center gap-5 sm:flex-row sm:items-center">
          <div className="size-[168px] shrink-0">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Pie
                  data={slices}
                  dataKey="value"
                  nameKey="label"
                  innerRadius={54}
                  outerRadius={82}
                  paddingAngle={slices.length > 1 ? 2 : 0}
                  stroke="none"
                  isAnimationActive={false}
                />
                <Tooltip content={(props) => <MoneyTooltip {...props} label="" />} />
              </PieChart>
            </ResponsiveContainer>
          </div>
          <ul className="flex w-full min-w-0 flex-col">
            {slices.map((slice) => (
              <li key={slice.category} className="flex items-center gap-2.5 border-b border-line py-2 last:border-0">
                <Dot color={slice.fill} />
                <span className="min-w-0 flex-1 truncate">{slice.label}</span>
                <span className="num text-muted">{pct(slice.pct, 1)}</span>
                <span className="num w-[7.5rem] text-right font-medium">{money(slice.value)}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Panel>
  )
}

function BarList({ rows, empty }: { rows: { label: string; value: number; color?: string }[]; empty: string }) {
  const sorted = [...rows].sort((a, b) => b.value - a.value)
  const max = Math.max(...sorted.map((row) => Math.abs(row.value)), 0)
  if (sorted.length === 0) return <p className="text-muted">{empty}</p>
  return (
    <ul className="flex flex-col gap-3">
      {sorted.map((row, i) => (
        <li key={`${row.label}-${i}`}>
          <div className="flex items-baseline justify-between gap-3">
            <span className="min-w-0 truncate">{row.label}</span>
            <span className="num font-medium">{money(row.value)}</span>
          </div>
          <div className="mt-1.5 h-1.5 rounded-full bg-surface-2">
            <div
              className="h-full rounded-full"
              style={{
                width: `${max > 0 ? Math.max(2, (Math.abs(row.value) / max) * 100) : 0}%`,
                background: row.color ?? 'var(--c5)',
              }}
            />
          </div>
        </li>
      ))}
    </ul>
  )
}

export default function OverviewPage() {
  const overview = useOverview()
  const members = useMembers()
  const { memberId } = useView()
  const viewName = memberId ? members.data?.find((member) => member.id === memberId)?.name : null

  if (overview.isPending) {
    return (
      <>
        <PageHeader title="Vue d’ensemble" />
        <LoadingBlock rows={4} />
      </>
    )
  }
  if (overview.isError) {
    return (
      <>
        <PageHeader title="Vue d’ensemble" />
        <ErrorState error={overview.error} onRetry={() => void overview.refetch()} />
      </>
    )
  }

  const data = overview.data
  const isEmpty = data.gross_assets === 0 && data.liabilities === 0
  const gainPct = data.invested > 0 ? (data.gain / data.invested) * 100 : null

  return (
    <>
      <PageHeader
        title="Vue d’ensemble"
        description={viewName ? `Part détenue par ${viewName}.` : 'Ensemble du foyer, toutes parts confondues.'}
      />

      {isEmpty ? (
        <EmptyState
          title="Aucun compte pour l’instant"
          action={
            <Link
              to="/comptes"
              className="inline-flex min-h-10 items-center rounded-[9px] bg-accent px-4 font-medium text-accent-ink"
            >
              Ajouter un compte
            </Link>
          }
        >
          Ajoutez vos comptes courants, livrets, placements, biens immobiliers et prêts : le patrimoine net, la
          répartition et l’historique se calculent à partir d’eux.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-4 sm:gap-5">
          <section className="panel px-4 py-5 sm:px-7 sm:py-7">
            <p className="text-muted">Patrimoine net</p>
            <p className="num mt-1 font-display text-[42px] leading-none font-semibold tracking-tight sm:text-[60px]">
              {money(data.net_worth)}
            </p>
            <div className="mt-6">
              <StrataBar allocation={data.allocation} />
            </div>
            <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-5 md:grid-cols-4">
              <Stat label="Actifs bruts" value={money(data.gross_assets)} />
              <Stat label="Dettes" value={money(data.liabilities)} />
              <Stat label="Actifs financiers" value={money(data.financial_assets)} sub="Épargne, titres et crypto" />
              <Stat
                label="Plus-value latente"
                value={money(data.gain, { signed: true })}
                tone={data.gain > 0 ? 'pos' : data.gain < 0 ? 'neg' : undefined}
                sub={
                  gainPct !== null
                    ? `${pct(gainPct, 1, true)} sur ${money(data.invested)} investis`
                    : 'Renseignez les prix d’achat pour la calculer'
                }
              />
            </div>
          </section>

          <HistoryPanel />

          <div className="grid gap-4 sm:gap-5 lg:grid-cols-2">
            <AllocationPanel allocation={data.allocation} />

            <Panel title="Par membre">
              {data.members.length === 0 ? (
                <p className="text-muted">Aucun membre ne détient de compte dans cette vue.</p>
              ) : (
                <ul className="flex flex-col">
                  {data.members.map((member) => (
                    <li key={member.member_id} className="flex items-center gap-3 border-b border-line py-2.5 last:border-0">
                      <Dot color={member.color} />
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-medium">{member.name}</div>
                        <div className="num text-[13px] text-muted">
                          {money(member.gross_assets)} d’actifs
                          {member.liabilities > 0 ? `, ${money(member.liabilities)} de dettes` : ''}
                        </div>
                      </div>
                      <span className="num font-display text-[17px] font-semibold">{money(member.net_worth)}</span>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>

            <Panel title="Par établissement">
              <BarList
                empty="Indiquez l’établissement de vos comptes pour voir cette répartition."
                rows={data.by_institution.map((row, i) => ({
                  label: row.institution || 'Non renseigné',
                  value: row.value,
                  color: SERIES_COLORS[i % SERIES_COLORS.length],
                }))}
              />
            </Panel>

            <Panel title="Par enveloppe">
              <BarList
                empty="Aucune enveloppe fiscale renseignée sur vos comptes."
                rows={data.by_envelope.map((row) => ({ label: envelopeLabel(row.envelope), value: row.value }))}
              />
            </Panel>
          </div>

          <Panel
            title="Principales positions"
            flush
            action={
              data.weighted_expense_ratio !== null ? (
                <span className="text-[13.5px] text-muted">
                  Frais courants moyens : <span className="num text-ink">{pctFromFraction(data.weighted_expense_ratio, 2)}</span>
                </span>
              ) : undefined
            }
          >
            {data.top_holdings.length === 0 ? (
              <p className="px-4 pb-5 text-muted sm:px-5">
                Ajoutez des positions (titres, fonds, crypto) dans vos comptes d’investissement pour les voir ici.
              </p>
            ) : (
              <div className="table-wrap">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Position</th>
                      <th className="max-sm:hidden">Compte</th>
                      <th className="r">Valeur</th>
                      <th className="r">Performance</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.top_holdings.map((holding, i) => (
                      <tr key={`${holding.name}-${i}`}>
                        <td>
                          <div className="font-medium">{holding.name}</div>
                          <div className="text-[13px] text-muted">
                            {holding.symbol ? <span>{holding.symbol}</span> : null}
                            <span className="sm:hidden">
                              {holding.symbol ? ', ' : ''}
                              {holding.account_name}
                            </span>
                          </div>
                        </td>
                        <td className="text-muted max-sm:hidden">{holding.account_name}</td>
                        <td className="r font-medium">{money(holding.value)}</td>
                        <td className="r">
                          <Signed value={holding.gain_pct}>{pct(holding.gain_pct, 1, true)}</Signed>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Panel>
        </div>
      )}
    </>
  )
}
