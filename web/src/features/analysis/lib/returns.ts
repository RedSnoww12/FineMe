// Rendement des placements (TWR, MWR) et comparaison à un indice : préparation de l'affichage.
// Les mesures elles-mêmes sont calculées par l'API ; ces fonctions ne font que les présenter.
import type { BenchmarkInput, BenchmarkKind, Performance } from '@/shared/api/types'
import { pctFromFraction } from '@/shared/lib/format'
import { periodError, type Period } from './period'

const PRESET_MONTHS = { '1m': 1, '3m': 3, '1y': 12 } as const

function shiftMonths(isoDate: string, months: number): string {
  const [year, month, day] = isoDate.split('-').map(Number)
  const target = new Date(Date.UTC(year, month - 1 - months, 1))
  const lastDay = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate()
  target.setUTCDate(Math.min(day, lastDay))
  return target.toISOString().slice(0, 10)
}

/** Bornes demandées à l'API pour la période choisie ; null si la période personnalisée est invalide. */
export function performanceBounds(period: Period, today: string): { from?: string; to?: string } | null {
  if (period.kind === 'custom') return periodError(period) ? null : { from: period.from, to: period.to }
  if (period.range === 'all') return {}
  return { from: shiftMonths(today, PRESET_MONTHS[period.range]) }
}

export type MeasureKey = 'twr' | 'twr_annualized' | 'mwr' | 'mwr_annualized'

/** Chaque mesure, expliquée en une phrase sans jargon. */
export const MEASURES: { key: MeasureKey; label: string; explanation: string }[] = [
  {
    key: 'twr',
    label: 'Performance des placements',
    explanation:
      'Ce qu’ont rapporté vos placements eux-mêmes, sans tenir compte du montant ni du moment de vos versements : c’est le chiffre à comparer à un indice.',
  },
  {
    key: 'twr_annualized',
    label: 'Par an, en moyenne',
    explanation: 'La performance des placements ramenée à une année, pour comparer des durées différentes.',
  },
  {
    key: 'mwr',
    label: 'Rendement de votre argent',
    explanation:
      'Ce qu’a rapporté l’argent que vous avez réellement placé : verser juste avant une hausse l’améliore, juste avant une baisse le dégrade.',
  },
  {
    key: 'mwr_annualized',
    label: 'Rendement de votre argent par an',
    explanation: 'Le rendement de votre argent ramené à une année, comme le taux d’un livret.',
  },
]

export const YEARLY_EXPLANATION =
  'La performance des placements de chaque année civile, du 1er janvier au 31 décembre ou sur la partie couverte par l’historique.'
export const CURVE_EXPLANATION =
  'Ce que seraient devenus 100 € placés au début de la période, comparé au même montant placé dans l’indice choisi.'

/** Pourquoi une mesure manque. */
export function undefinedReason(key: MeasureKey): string {
  return key.endsWith('_annualized')
    ? 'Affiché à partir d’un an d’historique : annualiser une période plus courte exagère le résultat.'
    : 'Indéfini : il faut au moins deux relevés de l’historique avec de l’argent placé.'
}

/** Mesure en pourcentage signé, ou « Indéfini ». */
export function formatMeasure(value: number | null): string {
  return value === null ? 'Indéfini' : pctFromFraction(value, 1, true)
}

/** Vrai si l'année n'est couverte qu'en partie par l'historique. */
export function isPartialYear(year: Performance['years'][number]): boolean {
  return year.from > `${year.year - 1}-12-31` || year.to < `${year.year}-12-31`
}

/** Années de la plus récente à la plus ancienne. */
export function yearsNewestFirst(performance: Performance): Performance['years'] {
  return [...performance.years].sort((a, b) => b.year - a.year)
}

export type CurveRow = { date: string; portfolio: number; benchmark: number | null }

export function curveRows(performance: Performance): CurveRow[] {
  return performance.points.map((point) => ({ date: point.date, portfolio: point.value, benchmark: point.benchmark }))
}

export type BenchmarkForm = { name: string; symbol: string; isin: string; kind: BenchmarkKind; currency: string }

export const EMPTY_BENCHMARK_FORM: BenchmarkForm = { name: '', symbol: '', isin: '', kind: 'security', currency: 'EUR' }

/** Corps de la requête, ou le message d'erreur à afficher. */
export function benchmarkInput(form: BenchmarkForm): BenchmarkInput | string {
  const name = form.name.trim()
  const symbol = form.symbol.trim()
  const isin = form.isin.trim().toUpperCase()
  const currency = form.currency.trim().toUpperCase()
  if (!name) return 'Donnez un nom à l’indice.'
  if (!symbol && !isin) return 'Indiquez un ticker ou un ISIN pour suivre l’indice.'
  if (!/^[A-Z]{3}$/.test(currency)) return 'Devise invalide : code à trois lettres (EUR, USD…).'
  return {
    name,
    kind: form.kind,
    currency,
    ...(symbol ? { symbol } : {}),
    ...(isin ? { isin } : {}),
  }
}
