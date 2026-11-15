// Calculs des simulateurs. Fonctions pures, sans dépendance au navigateur.
// Conventions : taux en pourcentage annuel (7 = 7 %), versements en fin de mois,
// capitalisation mensuelle au taux équivalent ((1 + r)^(1/12) − 1).

/** Taux mensuel équivalent à un taux annuel exprimé en pourcentage. */
export function monthlyRate(annualPct: number): number {
  return Math.pow(1 + annualPct / 100, 1 / 12) - 1
}

/** Valeur future d'un capital initial et de versements mensuels (fin de mois). */
export function futureValue(initial: number, monthly: number, annualPct: number, months: number): number {
  const n = Math.max(0, Math.round(months))
  const r = monthlyRate(annualPct)
  if (r === 0) return initial + monthly * n
  const growth = Math.pow(1 + r, n)
  return initial * growth + monthly * ((growth - 1) / r)
}

