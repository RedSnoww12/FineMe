import { useEffect, useState } from 'react'
import { retryAfterOf } from '@/shared/api/client'
import { remainingSeconds } from '../lib/cooldown'

/**
 * Secondes restantes avant un nouvel essai après un refus 429 (`Retry-After`) ; 0 = essai possible.
 * `submittedAt` : instant d'envoi de la requête refusée (ms), point de départ de l'attente.
 */
export function useCooldown(error: unknown, submittedAt: number): number {
  const seconds = retryAfterOf(error)
  const deadline = seconds && submittedAt ? submittedAt + seconds * 1000 : null
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (deadline === null) return
    const tick = () => setNow(Date.now())
    const first = window.setTimeout(tick, 0)
    const timer = window.setInterval(tick, 1000)
    return () => {
      window.clearTimeout(first)
      window.clearInterval(timer)
    }
  }, [deadline])

  // Avant le premier battement, `now` peut dater du montage : l'attente part au plus tôt de l'envoi.
  return remainingSeconds(deadline, Math.max(now, submittedAt))
}
