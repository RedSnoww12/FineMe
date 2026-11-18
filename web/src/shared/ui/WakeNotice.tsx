import { useIsFetching, useIsMutating } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

export const WAKE_DELAY_MS = 5_000
export const WAKE_MESSAGE = 'Réveil du serveur en cours, cela peut prendre jusqu’à une minute…'

/** Prévient quand une requête traîne : sur l'offre gratuite de l'hébergeur, l'API endormie met du temps à répondre. */
export function WakeNotice() {
  const busy = useIsFetching() + useIsMutating() > 0
  const [slow, setSlow] = useState(false)

  useEffect(() => {
    if (!busy) return setSlow(false)
    const timer = setTimeout(() => setSlow(true), WAKE_DELAY_MS)
    return () => clearTimeout(timer)
  }, [busy])

  return (
    <div role="status" aria-live="polite" className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      {slow ? <p className="panel px-4 py-2.5 text-[14px] text-muted shadow-lg">{WAKE_MESSAGE}</p> : null}
    </div>
  )
}
