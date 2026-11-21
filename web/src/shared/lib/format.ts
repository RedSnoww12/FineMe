const LOCALE = 'fr-FR'

const currencyFormatters = new Map<string, Intl.NumberFormat>()

function currencyFormatter(currency: string, decimals: number) {
  const key = `${currency}:${decimals}`
  let formatter = currencyFormatters.get(key)
  if (!formatter) {
    try {
      formatter = new Intl.NumberFormat(LOCALE, {
        style: 'currency',
        currency,
        minimumFractionDigits: decimals,
        maximumFractionDigits: decimals,
      })
    } catch {
      // code devise inconnu : on retombe sur un nombre suivi du code
      formatter = new Intl.NumberFormat(LOCALE, { minimumFractionDigits: decimals, maximumFractionDigits: decimals })
    }
    currencyFormatters.set(key, formatter)
  }
  return formatter
}

/** Ce qui remplace les chiffres d'un montant en mode discret. */
export const MASK = '•••••'

// Mode discret : un seul interrupteur pour tous les montants de l'application, piloté par `DiscreetProvider`.
let amountsHidden = false

export function setAmountsHidden(hidden: boolean) {
  amountsHidden = hidden
}

export function areAmountsHidden(): boolean {
  return amountsHidden
}

/** `••••• €` : garde le symbole et sa place selon la devise, sans rien laisser deviner (ni signe, ni ordre de grandeur). */
function maskedMoney(currency: string): string {
  let masked = ''
  for (const part of currencyFormatter(currency, 0).formatToParts(0)) {
    if (part.type === 'integer') masked += MASK
    else if (part.type === 'currency' || part.type === 'literal') masked += part.value
  }
  return masked
}

type MoneyOptions = {
  decimals?: number
  currency?: string
  signed?: boolean
  /** Masque le montant ; par défaut, suit le mode discret. */
  masked?: boolean
}

/** Montant en euros par défaut, sans décimales : `12 345 €`. */
export function money(value: number | null | undefined, options: MoneyOptions = {}): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '–'
  const { decimals = 0, currency = 'EUR', signed = false, masked = amountsHidden } = options
  if (masked) return maskedMoney(currency)
  const rounded = Math.abs(value) < 0.5 * Math.pow(10, -decimals) ? 0 : value
  const text = currencyFormatter(currency, decimals).format(rounded)
  return signed && rounded > 0 ? `+${text}` : text
}

/** Montant compact pour les axes de graphiques : `12 k€`, `1,2 M€`. */
export function moneyCompact(value: number, masked = amountsHidden): string {
  if (masked) return maskedMoney('EUR')
  return new Intl.NumberFormat(LOCALE, {
    style: 'currency',
    currency: 'EUR',
    notation: 'compact',
    // Minimum explicite : sinon l'euro impose ses décimales (« 12,0 k € ») selon la version d'ICU du moteur.
    minimumFractionDigits: 0,
    maximumFractionDigits: 1,
  }).format(value)
}

/** `value` déjà en pourcentage (7 = 7 %). */
export function pct(value: number | null | undefined, decimals = 1, signed = false): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '–'
  const text = new Intl.NumberFormat(LOCALE, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value)
  return `${signed && value > 0 ? '+' : ''}${text} %`
}

/** `fraction` entre 0 et 1 (0,5 = 50 %). */
export function pctFromFraction(fraction: number | null | undefined, decimals = 1, signed = false): string {
  if (fraction === null || fraction === undefined) return '–'
  return pct(fraction * 100, decimals, signed)
}

export function number(value: number | null | undefined, maxDecimals = 4): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '–'
  return new Intl.NumberFormat(LOCALE, { maximumFractionDigits: maxDecimals }).format(value)
}

function toDate(value: string): Date | null {
  // `AAAA-MM-JJ` est lu en heure locale pour éviter le décalage d'un jour.
  const dateOnly = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  const date = dateOnly
    ? new Date(Number(dateOnly[1]), Number(dateOnly[2]) - 1, Number(dateOnly[3]))
    : new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

export function date(value: string | null | undefined, style: 'short' | 'long' | 'month' = 'short'): string {
  if (!value) return '–'
  const d = toDate(value)
  if (!d) return '–'
  const options: Intl.DateTimeFormatOptions =
    style === 'long'
      ? { day: 'numeric', month: 'long', year: 'numeric' }
      : style === 'month'
        ? { month: 'short', year: 'numeric' }
        : { day: '2-digit', month: '2-digit', year: 'numeric' }
  return new Intl.DateTimeFormat(LOCALE, options).format(d)
}

export function dateTime(value: string | null | undefined): string {
  if (!value) return '–'
  const d = toDate(value)
  if (!d) return '–'
  return new Intl.DateTimeFormat(LOCALE, { dateStyle: 'medium', timeStyle: 'short' }).format(d)
}

/** `14 ans et 3 mois`. */
export function duration(months: number): string {
  const years = Math.floor(months / 12)
  const rest = months % 12
  const parts: string[] = []
  if (years > 0) parts.push(`${years} an${years > 1 ? 's' : ''}`)
  if (rest > 0 || years === 0) parts.push(`${rest} mois`)
  return parts.join(' et ')
}

/** Valeur d'un champ numérique saisi en français, pour préremplir un formulaire. */
export function toInput(value: number | null | undefined, maxDecimals = 6): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return ''
  return new Intl.NumberFormat(LOCALE, { maximumFractionDigits: maxDecimals, useGrouping: false }).format(value)
}

/** `21/11 14:05` : horodatage court des données affichées hors ligne. */
export function shortDateTime(timestamp: number): string {
  // Assemblé à la main : la ponctuation entre date et heure varie selon la version d'ICU du moteur.
  const d = new Date(timestamp)
  const two = (n: number) => String(n).padStart(2, '0')
  return `${two(d.getDate())}/${two(d.getMonth() + 1)} ${two(d.getHours())}:${two(d.getMinutes())}`
}

export function todayIso(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
}
