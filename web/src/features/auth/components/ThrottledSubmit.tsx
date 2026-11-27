import type { ReactNode } from 'react'
import { useCooldown } from '../hooks/useCooldown'
import { formatWait } from '../lib/cooldown'
import { retryAfterOf } from '@/shared/api/client'
import { Button, FormError } from '@/shared/ui/ui'

/** Erreur et bouton d'envoi ; après un 429, le bouton reste désactivé et affiche le temps d'attente. */
export function ThrottledSubmit({
  error,
  submittedAt,
  busy,
  disabled = false,
  children,
}: {
  error: unknown
  submittedAt: number
  busy: boolean
  disabled?: boolean
  children: ReactNode
}) {
  const wait = useCooldown(error, submittedAt)
  const throttledAndOver = retryAfterOf(error) !== null && wait === 0
  return (
    <>
      {throttledAndOver ? null : <FormError error={error} />}
      <Button type="submit" variant="primary" busy={busy} disabled={disabled || wait > 0}>
        {wait > 0 ? `Réessayer dans ${formatWait(wait)}` : children}
      </Button>
    </>
  )
}
