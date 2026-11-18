// Répartition des actifs bruts selon un axe : catégorie, enveloppe, établissement ou membre.
import type { Overview } from '@/shared/api/types'
import { CATEGORY_COLOR, envelopeLabel, SERIES_COLORS } from '@/shared/lib/labels'

export type Dimension = 'category' | 'envelope' | 'institution' | 'member'

export const DIMENSION_LABEL: Record<Dimension, string> = {
  category: 'Catégorie',
  envelope: 'Enveloppe',
  institution: 'Établissement',
  member: 'Membre',
}

type BreakdownRow = { key: string; label: string; value: number; color?: string }

export type Slice = { key: string; label: string; value: number; pct: number; color: string }

export const OTHERS_KEY = 'autres'
/** Écart en dessous duquel le reste « sans enveloppe » est un arrondi, pas un montant. */
const ROUNDING = 0.5

export function breakdownRows(overview: Overview, dimension: Dimension): BreakdownRow[] {
  switch (dimension) {
    case 'category':
      return overview.allocation.map((slice) => ({
        key: slice.category,
        label: slice.label,
        value: slice.value,
        color: CATEGORY_COLOR[slice.category],
      }))
    case 'envelope': {
      const rows = overview.by_envelope.map((row) => ({ key: row.envelope, label: envelopeLabel(row.envelope), value: row.value }))
      // Les actifs sans enveloppe complètent la répartition jusqu'aux actifs bruts.
      const rest = overview.gross_assets - rows.reduce((sum, row) => sum + row.value, 0)
      return rest > ROUNDING ? [...rows, { key: 'sans-enveloppe', label: envelopeLabel(null), value: rest }] : rows
    }
    case 'institution':
      return overview.by_institution.map((row) => ({
        key: row.institution,
        label: row.institution || 'Non renseigné',
        value: row.value,
      }))
    case 'member':
      return overview.members.map((member) => ({
        key: member.member_id,
        label: member.name,
        value: member.gross_assets,
        color: member.color,
      }))
  }
}

/**
 * Parts d'un graphique en anneau : valeurs positives, triées de la plus grande à la plus petite, les plus
 * petites regroupées en « Autres » au-delà de `maxSlices` pour rester lisibles. `pct` somme à 100.
 */
export function toSlices(rows: readonly BreakdownRow[], maxSlices = 6): Slice[] {
  const positive = rows.filter((row) => row.value > 0).sort((a, b) => b.value - a.value)
  const total = positive.reduce((sum, row) => sum + row.value, 0)
  if (total === 0) return []
  const kept = positive.length > maxSlices ? positive.slice(0, maxSlices - 1) : positive
  const grouped = positive.slice(kept.length)
  const slices = kept.map((row, i) => ({
    key: row.key,
    label: row.label,
    value: row.value,
    pct: (row.value / total) * 100,
    color: row.color ?? SERIES_COLORS[i % SERIES_COLORS.length],
  }))
  if (grouped.length === 0) return slices
  const othersValue = grouped.reduce((sum, row) => sum + row.value, 0)
  return [
    ...slices,
    { key: OTHERS_KEY, label: 'Autres', value: othersValue, pct: (othersValue / total) * 100, color: 'var(--c7)' },
  ]
}
