import { useId, type ReactNode } from 'react'
import { AXIS_TICK, type TooltipProps } from '@/shared/ui/charts'
import { Button, Dot, ErrorState, Skeleton } from '@/shared/ui/ui'
import { date as formatDate } from '@/shared/lib/format'
import { SMOOTH_DAYS, type ChartSettings } from '../lib/chartSettings'
import { formatUnit, type Unit } from '../lib/period'

export { AXIS_TICK }

/** Hauteur commune des graphiques : lisible sur téléphone sans faire défiler. */
export const CHART_HEIGHT = 'h-[240px] sm:h-[280px]'

/** Graduation d'un axe de dates : jour et mois sur une période courte, mois et année au-delà de trois mois. */
export function dateTick(firstDate: string | undefined, lastDate: string | undefined) {
  const span = firstDate && lastDate ? (Date.parse(lastDate) - Date.parse(firstDate)) / 86_400_000 : 0
  return (value: string) => (span > 95 ? formatDate(value, 'month') : formatDate(value).slice(0, 5))
}

/** Explication en langage courant, sous le titre d'un graphique. */
export function ChartIntro({ children }: { children: ReactNode }) {
  return <p className="mb-3 max-w-[70ch] text-[13.5px] text-muted">{children}</p>
}

/**
 * Zone de graphique : le dessin est une image décrite par `summary` pour les lecteurs d'écran ; les données
 * exactes restent accessibles au clavier par le tableau qui suit.
 */
export function ChartFigure({ summary, children, className = CHART_HEIGHT }: { summary: string; children: ReactNode; className?: string }) {
  return (
    <div role="img" aria-label={summary} className={className}>
      {children}
    </div>
  )
}

/** État de la liste des comptes, dont dépendent plusieurs panneaux. */
export type AccountsState = { isPending: boolean; error: unknown; onRetry: () => void }

/** Chargement, erreur ou état vide d'un panneau ; le contenu sinon. */
export function PanelState({
  isPending,
  error,
  onRetry,
  empty,
  children,
}: {
  isPending: boolean
  error: unknown
  onRetry?: () => void
  /** Message affiché quand il n'y a rien à tracer ; null si les données suffisent. */
  empty: ReactNode | null
  children: () => ReactNode
}) {
  if (isPending) return <Skeleton className="h-[240px]" />
  if (error) return <ErrorState error={error} onRetry={onRetry} />
  if (empty) return <p className="py-6 text-muted">{empty}</p>
  return <>{children()}</>
}

export type Column = { header: string; numeric?: boolean }

/** Alternative tabulaire d'un graphique, repliée par défaut, navigable au clavier. */
export function DataTable({ caption, columns, rows }: { caption: string; columns: Column[]; rows: ReactNode[][] }) {
  const id = useId()
  return (
    <details className="mt-3 rounded-[10px] border border-line">
      <summary className="flex min-h-10 items-center px-3 text-[13.5px] font-medium" aria-controls={id}>
        Voir les données en tableau
      </summary>
      <div id={id} className="table-wrap max-h-[360px] overflow-y-auto border-t border-line">
        <table className="table">
          <caption className="sr-only">{caption}</caption>
          <thead>
            <tr>
              {columns.map((column) => (
                <th key={column.header} scope="col" className={column.numeric ? 'r' : undefined}>
                  {column.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((cells, i) => (
              <tr key={i}>
                {cells.map((cell, j) => (
                  <td key={j} className={columns[j]?.numeric ? 'r' : undefined}>
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  )
}

type Series = { key: string; label: string; color: string }

/** Infobulle d'un graphique daté : une ligne par série, au format de l'unité choisie. */
export function UnitTooltip({
  active,
  payload,
  label,
  unit,
  series,
  note,
  formatLabel = (value) => formatDate(value, 'long'),
}: TooltipProps & {
  unit: Unit
  series: Series[]
  note?: (label: string) => ReactNode
  formatLabel?: (label: string) => string
}) {
  if (!active || !payload || payload.length === 0) return null
  const key = String(label ?? '')
  const values = new Map<string, number>()
  for (const entry of payload) {
    if (typeof entry.value === 'number') values.set(String(entry.dataKey ?? ''), entry.value)
  }
  const shown = series.filter((s) => values.has(s.key))
  if (shown.length === 0) return null
  return (
    <div className="rounded-[10px] border border-line bg-surface px-3 py-2 text-[13px] shadow-[0_6px_20px_rgb(0_0_0/0.14)]">
      <div className="mb-1 font-medium">{formatLabel(key)}</div>
      <ul className="flex flex-col gap-0.5">
        {shown.map((s) => (
          <li key={s.key} className="flex items-center justify-between gap-4">
            <span className="flex items-center gap-1.5 text-muted">
              <Dot color={s.color} />
              {s.label}
            </span>
            <span className="num font-medium">{formatUnit(values.get(s.key) ?? 0, unit)}</span>
          </li>
        ))}
      </ul>
      {note?.(key)}
    </div>
  )
}

function Toggle({ pressed, onClick, disabled, title, children }: { pressed: boolean; onClick: () => void; disabled?: boolean; title?: string; children: ReactNode }) {
  return (
    <Button size="sm" variant={pressed ? 'primary' : 'secondary'} aria-pressed={pressed} onClick={onClick} disabled={disabled} title={title}>
      {children}
    </Button>
  )
}

/**
 * Réglages d'un graphique daté : lissage, échelle logarithmique (si `log` est fourni) et réinitialisation du
 * zoom. L'échelle logarithmique est désactivée quand une série tracée est nulle ou négative.
 */
export function ChartControls({
  settings,
  update,
  log,
  zoom,
}: {
  settings: ChartSettings
  update: (change: (current: ChartSettings) => Partial<ChartSettings>) => void
  log?: { available: boolean }
  zoom: { zoomed: boolean; reset: () => void }
}) {
  return (
    <div role="group" aria-label="Affichage du graphique" className="mb-3 flex flex-wrap gap-2">
      <Toggle pressed={settings.smooth} onClick={() => update((s) => ({ smooth: !s.smooth }))}>
        Lisser ({SMOOTH_DAYS} j)
      </Toggle>
      {log ? (
        <Toggle
          pressed={settings.log && log.available}
          disabled={!log.available}
          title={log.available ? undefined : 'Indisponible : une série affichée est nulle ou négative, ou les valeurs sont en %.'}
          onClick={() => update((s) => ({ log: !s.log }))}
        >
          Échelle log
        </Toggle>
      ) : null}
      <Button size="sm" variant="ghost" onClick={zoom.reset} disabled={!zoom.zoomed}>
        Réinitialiser le zoom
      </Button>
    </div>
  )
}
