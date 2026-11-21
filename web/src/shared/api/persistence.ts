import { createSyncStoragePersister } from '@tanstack/query-sync-storage-persister'
import type { Persister } from '@tanstack/react-query-persist-client'

/**
 * Copie locale des dernières réponses consultées, pour l'affichage hors ligne. Elle reste dans le navigateur de
 * l'appareil (jamais dans le cache du service worker), expire vite et est effacée à la déconnexion.
 */
export const PERSISTED_CACHE_KEY = 'fineme.cache'

/** Au-delà, la copie locale est ignorée puis remplacée : les données hors ligne ne vieillissent pas indéfiniment. */
export const PERSISTED_CACHE_MAX_AGE_MS = 24 * 60 * 60 * 1000

/** Change quand la forme des réponses change, pour ne jamais réhydrater un cache incompatible. */
export const PERSISTED_CACHE_BUSTER = 'v1'

type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>

function defaultStorage(): KeyValueStorage | undefined {
  try {
    return typeof window === 'undefined' ? undefined : window.localStorage
  } catch {
    return undefined
  }
}

export function createCachePersister(storage: KeyValueStorage | undefined = defaultStorage()): Persister {
  return createSyncStoragePersister({ storage, key: PERSISTED_CACHE_KEY })
}

export function purgePersistedCache(storage: KeyValueStorage | undefined = defaultStorage()) {
  try {
    storage?.removeItem(PERSISTED_CACHE_KEY)
  } catch {
    // stockage indisponible : rien n'a pu y être écrit
  }
}

type DisplayedQuery = { dataUpdatedAt: number; observed: boolean }

/**
 * Date des données affichées hors ligne : la plus ancienne parmi celles à l'écran (c'est la plus pessimiste),
 * à défaut la plus récente du cache. `null` quand rien n'a encore été chargé.
 */
export function offlineDataTimestamp(queries: DisplayedQuery[]): number | null {
  const loaded = queries.filter((query) => query.dataUpdatedAt > 0)
  const shown = loaded.filter((query) => query.observed)
  if (shown.length > 0) return Math.min(...shown.map((query) => query.dataUpdatedAt))
  if (loaded.length > 0) return Math.max(...loaded.map((query) => query.dataUpdatedAt))
  return null
}
