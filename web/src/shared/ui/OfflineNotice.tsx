import { onlineManager, useQueryClient } from '@tanstack/react-query'
import { useCallback, useSyncExternalStore } from 'react'
import { offlineDataTimestamp } from '@/shared/api/persistence'
import { shortDateTime } from '@/shared/lib/format'

function useIsOnline() {
  return useSyncExternalStore(
    (onChange) => onlineManager.subscribe(onChange),
    () => onlineManager.isOnline(),
  )
}

function useOfflineDataTimestamp() {
  const queryCache = useQueryClient().getQueryCache()
  const subscribe = useCallback((onChange: () => void) => queryCache.subscribe(onChange), [queryCache])
  return useSyncExternalStore(subscribe, () =>
    offlineDataTimestamp(
      queryCache.getAll().map((query) => ({ dataUpdatedAt: query.state.dataUpdatedAt, observed: query.getObserversCount() > 0 })),
    ),
  )
}

export function offlineMessage(timestamp: number | null): string {
  return timestamp === null
    ? 'Hors ligne : aucune donnée enregistrée sur cet appareil.'
    : `Hors ligne, données du ${shortDateTime(timestamp)}`
}

/** Bandeau hors ligne : les écrans affichent alors la dernière copie locale, sans pouvoir l'actualiser. */
export function OfflineNotice() {
  const online = useIsOnline()
  const timestamp = useOfflineDataTimestamp()
  return (
    <div role="status" aria-live="polite">
      {online ? null : (
        <p className="bg-ink px-4 py-2 text-center text-[13.5px] font-medium text-surface">
          {offlineMessage(timestamp)}
        </p>
      )}
    </div>
  )
}
