// Réglages d'affichage d'un graphique daté : séries masquées, lissage et échelle logarithmique.

/** Fenêtre du lissage, en jours. */
export const SMOOTH_DAYS = 7

export type ChartSettings = { hidden: string[]; smooth: boolean; log: boolean }

export const DEFAULT_SETTINGS: ChartSettings = { hidden: [], smooth: false, log: false }

/** Relit des réglages mémorisés ; tout champ absent ou invalide reprend sa valeur par défaut. */
export function parseSettings(raw: string | null): ChartSettings {
  try {
    const value: unknown = raw ? JSON.parse(raw) : null
    if (typeof value !== 'object' || value === null) return DEFAULT_SETTINGS
    const { hidden, smooth, log } = value as Record<string, unknown>
    return {
      hidden: Array.isArray(hidden) ? hidden.filter((key): key is string => typeof key === 'string') : [],
      smooth: smooth === true,
      log: log === true,
    }
  } catch {
    return DEFAULT_SETTINGS
  }
}

/** Masque la série si elle est affichée, l'affiche sinon. */
export function toggleHidden(hidden: readonly string[], key: string): string[] {
  return hidden.includes(key) ? hidden.filter((k) => k !== key) : [...hidden, key]
}

/**
 * Moyenne glissante sur les `window` derniers jours. En début de série, la moyenne porte sur les jours
 * disponibles ; les valeurs manquantes sont ignorées et un jour sans valeur reste un trou.
 */
export function movingAverage(values: readonly (number | null)[], window: number): (number | null)[] {
  return values.map((value, i) => {
    if (value === null) return null
    const known = values.slice(Math.max(0, i - window + 1), i + 1).filter((v): v is number => v !== null)
    return known.reduce((sum, v) => sum + v, 0) / known.length
  })
}

/** Lisse les colonnes `keys` de chaque ligne, les autres champs restant inchangés. */
export function smoothRows<K extends string, T extends Record<K, number | null>>(rows: readonly T[], keys: readonly K[], window: number): T[] {
  const smoothed = keys.map((key) => movingAverage(rows.map((row) => row[key]), window))
  return rows.map((row, i) => {
    const next = { ...row }
    keys.forEach((key, k) => {
      next[key] = smoothed[k][i] as T[K]
    })
    return next
  })
}

/** Une échelle logarithmique n'a de sens que si toutes les valeurs tracées sont strictement positives. */
export function canUseLog<K extends string>(rows: readonly Record<K, number | null>[], keys: readonly K[]): boolean {
  return keys.length > 0 && rows.every((row) => keys.every((key) => row[key] === null || (row[key] ?? 0) > 0))
}
