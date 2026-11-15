// Lecture des saisies et fichiers au format français. Fonctions pures.

/** Lit un nombre au format français (`1 234,56`) ou anglais (`1,234.56`). */
export function parseFrNumber(raw: string | null | undefined): number | null {
  if (raw === null || raw === undefined) return null
  let s = String(raw).trim()
  if (!s) return null

  let negative = false
  if (/^\(.*\)$/.test(s)) {
    negative = true
    s = s.slice(1, -1)
  }
  // espaces (y compris insécables), symboles monétaires, apostrophes de milliers
  s = s.replace(/[\s   '’]/g, '').replace(/[€$£%]|EUR|USD/gi, '')
  s = s.replace(/−/g, '-')
  if (s.endsWith('-')) {
    negative = !negative
    s = s.slice(0, -1)
  }
  if (s.startsWith('-')) {
    negative = !negative
    s = s.slice(1)
  } else if (s.startsWith('+')) {
    s = s.slice(1)
  }
  if (!s) return null

  const lastComma = s.lastIndexOf(',')
  const lastDot = s.lastIndexOf('.')
  if (lastComma !== -1 && lastDot !== -1) {
    // le dernier séparateur rencontré est le séparateur décimal
    if (lastComma > lastDot) s = s.replace(/\./g, '').replace(',', '.')
    else s = s.replace(/,/g, '')
  } else if (lastComma !== -1) {
    const commas = s.split(',').length - 1
    s = commas > 1 ? s.replace(/,/g, '') : s.replace(',', '.')
  } else if (lastDot !== -1) {
    const dots = s.split('.').length - 1
    if (dots > 1) s = s.replace(/\./g, '')
  }

  if (!/^\d*\.?\d+$|^\d+\.$/.test(s)) return null
  const value = Number(s)
  if (!Number.isFinite(value)) return null
  return negative ? -value : value
}

function isValidDate(y: number, m: number, d: number) {
  const date = new Date(Date.UTC(y, m - 1, d))
  return date.getUTCFullYear() === y && date.getUTCMonth() === m - 1 && date.getUTCDate() === d
}

const pad = (n: number) => String(n).padStart(2, '0')

/** Lit `JJ/MM/AAAA` (ou `-`, `.`, année sur 2 chiffres) et `AAAA-MM-JJ`. Renvoie `AAAA-MM-JJ`. */
export function parseDate(raw: string | null | undefined): string | null {
  if (!raw) return null
  const s = String(raw).trim()
  let y: number, m: number, d: number

  const iso = /^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})(?:[T\s].*)?$/.exec(s)
  const fr = /^(\d{1,2})[-/.](\d{1,2})[-/.](\d{4}|\d{2})(?:\s.*)?$/.exec(s)
  if (iso) {
    y = Number(iso[1])
    m = Number(iso[2])
    d = Number(iso[3])
  } else if (fr) {
    d = Number(fr[1])
    m = Number(fr[2])
    y = Number(fr[3])
    if (fr[3].length === 2) y += y >= 70 ? 1900 : 2000
  } else {
    return null
  }
  if (!isValidDate(y, m, d)) return null
  return `${y}-${pad(m)}-${pad(d)}`
}

