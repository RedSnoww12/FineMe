/** Durée d'attente lisible : « 45 s » sous une minute, sinon minutes arrondies au-dessus (« 2 min »). */
export function formatWait(seconds: number): string {
  const whole = Math.max(0, Math.ceil(seconds))
  return whole < 60 ? `${whole} s` : `${Math.ceil(whole / 60)} min`
}

/** Secondes restantes avant `deadline` (ms depuis l'époque) à l'instant `now`, jamais négatives. */
export function remainingSeconds(deadline: number | null, now: number): number {
  if (deadline === null) return 0
  return Math.max(0, Math.ceil((deadline - now) / 1000))
}
