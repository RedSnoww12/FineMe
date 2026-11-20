import { useId } from 'react'
import { Area, CartesianGrid, ComposedChart, Line, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { AXIS_TICK, Legend, type TooltipProps } from '@/shared/ui/charts'
import { Dot } from '@/shared/ui/ui'
import { money, moneyCompact } from '@/shared/lib/format'
import type { FanPoint } from '../lib/monteCarlo'

const COLORS = { band: 'var(--c1)', median: 'var(--c1)', contributed: 'var(--c7)', target: 'var(--brass)' }

const SERIES = [
  { key: 'p90', label: 'Favorable (90e percentile)' },
  { key: 'p50', label: 'Médian (50e percentile)' },
  { key: 'p10', label: 'Défavorable (10e percentile)' },
] as const

function FanTooltip({ active, payload, label }: TooltipProps) {
  const point = payload?.[0] as { payload?: FanPoint } | undefined
  if (!active || !point?.payload) return null
  const row = point.payload
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      <div className="mb-1 font-medium">{String(label)}</div>
      <ul className="flex flex-col gap-0.5">
        {SERIES.map((series) => (
          <li key={series.key} className="flex items-center justify-between gap-4">
            <span className="flex items-center gap-1.5 text-muted">
              <Dot color={COLORS.median} />
              {series.label}
            </span>
            <span className="num font-medium">{money(row[series.key])}</span>
          </li>
        ))}
        <li className="flex items-center justify-between gap-4">
          <span className="flex items-center gap-1.5 text-muted">
            <Dot color={COLORS.contributed} />
            Total versé
          </span>
          <span className="num font-medium">{money(row.contributed)}</span>
        </li>
      </ul>
    </div>
  )
}

/**
 * Graphique en éventail : bande entre les percentiles 10 et 90, ligne médiane, total versé et capital cible.
 * Les couleurs viennent des variables du thème (clair ou sombre). Le graphique est doublé d'un tableau
 * (voir `FanTable`) pour les lecteurs d'écran.
 */
export function FanChart({ points, target, summary }: { points: FanPoint[]; target: number | null; summary: string }) {
  const data = points.map((point) => ({ ...point, band: [point.p10, point.p90] }))
  return (
    <figure>
      <figcaption className="sr-only">{summary}</figcaption>
      <div className="h-[240px] sm:h-[320px]" aria-hidden="true">
        <ResponsiveContainer width="100%" height="100%">
          <ComposedChart data={data} margin={{ top: 8, right: 6, bottom: 0, left: 0 }} accessibilityLayer={false}>
            <CartesianGrid stroke="var(--line)" vertical={false} />
            <XAxis
              dataKey="year"
              interval="preserveStartEnd"
              tick={AXIS_TICK}
              tickLine={false}
              axisLine={{ stroke: 'var(--line)' }}
              minTickGap={28}
            />
            <YAxis tickFormatter={(v: number) => moneyCompact(v)} tick={AXIS_TICK} tickLine={false} axisLine={false} width={62} />
            <Tooltip content={(props) => <FanTooltip {...props} />} />
            <Area
              dataKey="band"
              name="Entre les 10e et 90e percentiles"
              stroke="none"
              fill={COLORS.band}
              fillOpacity={0.18}
              isAnimationActive={false}
              activeDot={false}
            />
            {target !== null ? (
              <ReferenceLine
                y={target}
                stroke={COLORS.target}
                strokeDasharray="5 4"
                label={{ value: 'Cible', position: 'insideTopLeft', fill: COLORS.target, fontSize: 12 }}
              />
            ) : null}
            <Line
              dataKey="contributed"
              name="Total versé"
              stroke={COLORS.contributed}
              strokeDasharray="4 4"
              strokeWidth={1.5}
              dot={false}
              isAnimationActive={false}
            />
            <Line dataKey="p50" name="Médian" stroke={COLORS.median} strokeWidth={2.5} dot={false} isAnimationActive={false} />
          </ComposedChart>
        </ResponsiveContainer>
      </div>
      <Legend
        items={[
          { label: 'Médian', color: COLORS.median },
          { label: '8 trajectoires sur 10 dans la bande', color: 'color-mix(in srgb, var(--c1) 30%, transparent)' },
          { label: 'Total versé', color: COLORS.contributed },
          ...(target !== null ? [{ label: 'Capital cible', color: COLORS.target }] : []),
        ]}
      />
    </figure>
  )
}

/** Alternative accessible au graphique : les percentiles année par année, repliée par défaut. */
export function FanTable({ points }: { points: FanPoint[] }) {
  const id = useId()
  return (
    <details className="mt-3 rounded-[10px] border border-line">
      <summary className="flex min-h-10 items-center px-3 text-[13.5px] font-medium" aria-controls={id}>
        Voir les percentiles en tableau
      </summary>
      <div id={id} className="table-wrap max-h-[420px] overflow-y-auto border-t border-line">
        <table className="table">
          <caption className="sr-only">Capital simulé par année : percentiles 10, 50 et 90</caption>
          <thead className="sticky top-0 bg-surface">
            <tr>
              <th scope="col">Année</th>
              <th scope="col" className="r">Total versé</th>
              <th scope="col" className="r">Défavorable (10 %)</th>
              <th scope="col" className="r">Médian</th>
              <th scope="col" className="r">Favorable (90 %)</th>
            </tr>
          </thead>
          <tbody>
            {points.map((point) => (
              <tr key={point.year}>
                <td className="num">{point.year}</td>
                <td className="r text-muted">{money(point.contributed)}</td>
                <td className="r">{money(point.p10)}</td>
                <td className="r font-medium">{money(point.p50)}</td>
                <td className="r">{money(point.p90)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  )
}
