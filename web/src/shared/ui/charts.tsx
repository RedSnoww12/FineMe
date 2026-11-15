import type { ReactNode } from 'react'
import { money } from '@/shared/lib/format'
import { Dot } from '@/shared/ui/ui'

type TooltipEntry = { name?: unknown; value?: unknown; color?: string; dataKey?: unknown }
export type TooltipProps = {
  active?: boolean
  payload?: ReadonlyArray<TooltipEntry>
  label?: unknown
}

/** Infobulle commune aux graphiques : montants en euros, chiffres tabulaires. */
export function MoneyTooltip({
  active,
  payload,
  label,
  formatLabel,
}: TooltipProps & { formatLabel?: (label: unknown) => ReactNode }) {
  if (!active || !payload || payload.length === 0) return null
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      {label !== undefined && label !== '' ? (
        <div className="mb-1 font-medium">{formatLabel ? formatLabel(label) : String(label)}</div>
      ) : null}
      <ul className="flex flex-col gap-0.5">
        {payload.map((entry, i) => (
          <li key={i} className="flex items-center justify-between gap-4">
            <span className="flex items-center gap-1.5 text-muted">
              {entry.color ? <Dot color={entry.color} /> : null}
              {String(entry.name ?? '')}
            </span>
            <span className="num font-medium">{typeof entry.value === 'number' ? money(entry.value) : '–'}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export const AXIS_TICK = { fill: 'var(--muted)', fontSize: 12 }

/** Légende sous un graphique : une pastille par série. */
export function Legend({ items }: { items: { label: string; color: string }[] }) {
  return (
    <ul className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[13px] text-muted">
      {items.map((item) => (
        <li key={item.label} className="flex items-center gap-1.5">
          <Dot color={item.color} />
          {item.label}
        </li>
      ))}
    </ul>
  )
}
