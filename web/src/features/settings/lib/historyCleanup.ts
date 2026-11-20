// Nettoyage de l'historique : validation des saisies et sélection des points. Fonctions pures.
import type { HistoryCorrectionInput, HistoryReviewPoint } from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'

/** Mot exigé par l'API pour effacer tout l'historique (voir `docs/API.md`). */
export const RESET_WORD = 'EFFACER'

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/

/** Vrai si la saisie confirme la remise à zéro : le mot exact, espaces de début et de fin ignorés. */
export function isResetConfirmed(input: string): boolean {
  return input.trim() === RESET_WORD
}

/** Points du plus récent au plus ancien, pour la liste ; ne modifie pas l'entrée. */
export function newestFirst(points: readonly HistoryReviewPoint[]): HistoryReviewPoint[] {
  return [...points].sort((a, b) => b.date.localeCompare(a.date))
}

export function suspiciousPoints(points: readonly HistoryReviewPoint[]): HistoryReviewPoint[] {
  return points.filter((point) => point.suspicious)
}

/** Nombre de points enregistrés entre deux dates ISO, bornes incluses. */
export function countInRange(points: readonly HistoryReviewPoint[], from: string, to: string): number {
  return points.filter((point) => from <= point.date && point.date <= to).length
}

/** Le point du jour (ou d'un jour à venir) se recalcule, il ne se corrige pas. */
export function isCorrectable(point: HistoryReviewPoint, today: string): boolean {
  return point.date < today
}

export type RangeErrors = { from?: string; to?: string }

/** Erreurs de saisie d'une période à supprimer ; objet vide si elle est valide. */
export function rangeErrors(from: string, to: string): RangeErrors {
  const errors: RangeErrors = {}
  if (!ISO_DATE.test(from)) errors.from = 'Indiquez la date de début.'
  if (!ISO_DATE.test(to)) errors.to = 'Indiquez la date de fin.'
  if (!errors.from && !errors.to && from > to) errors.to = 'La date de fin doit suivre la date de début.'
  return errors
}

export type CorrectionErrors = { gross_assets?: string; liabilities?: string }

type CorrectionResult = { input: HistoryCorrectionInput; errors: null } | { input: null; errors: CorrectionErrors }

function amountError(value: number | null, messages: { missing: string; negative: string }): string | undefined {
  if (value === null || !Number.isFinite(value)) return messages.missing
  if (value < 0) return messages.negative
  return undefined
}

/** Lit les montants saisis (format français accepté) pour corriger un point. */
export function parseCorrection(grossRaw: string, liabilitiesRaw: string): CorrectionResult {
  const gross = parseFrNumber(grossRaw)
  const liabilities = parseFrNumber(liabilitiesRaw)
  const errors: CorrectionErrors = {}
  const grossError = amountError(gross, {
    missing: 'Indiquez les actifs bruts.',
    negative: 'Les actifs bruts ne peuvent pas être négatifs.',
  })
  const liabilitiesError = amountError(liabilities, {
    missing: 'Indiquez les dettes (0 si aucune).',
    negative: 'Les dettes ne peuvent pas être négatives.',
  })
  if (grossError) errors.gross_assets = grossError
  if (liabilitiesError) errors.liabilities = liabilitiesError
  if (gross === null || liabilities === null || grossError || liabilitiesError) return { input: null, errors }
  return { input: { gross_assets: gross, liabilities }, errors: null }
}
