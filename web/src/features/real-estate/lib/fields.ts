// Lecture des champs des simulateurs. Fonctions pures.

import { parseFrNumber } from '@/shared/lib/parse'

type ReadOptions = {
  /** Champ facultatif : vide vaut 0. */
  optional?: boolean
  /** Accepte une valeur négative (revalorisation en baisse, par exemple). */
  signed?: boolean
  max?: number
}

/** Nombre saisi, ou `null` s'il manque ou sort des bornes. */
export function readField(raw: string, options: ReadOptions = {}): number | null {
  if (!raw.trim()) return options.optional ? 0 : null
  const value = parseFrNumber(raw)
  if (value === null || !Number.isFinite(value)) return null
  if (!options.signed && value < 0) return null
  if (options.max !== undefined && value > options.max) return null
  return value
}

/** Toutes les valeurs, ou `null` dès qu'une manque : le simulateur n'affiche rien tant que la saisie est incomplète. */
export function readAll<K extends string>(values: Record<K, number | null>): Record<K, number> | null {
  for (const value of Object.values(values)) if (value === null) return null
  return values as Record<K, number>
}
