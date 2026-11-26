import { dehydrate, QueryClient } from '@tanstack/react-query'
import { persistQueryClientRestore, persistQueryClientSave } from '@tanstack/react-query-persist-client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createCachePersister,
  offlineDataTimestamp,
  PERSISTED_CACHE_BUSTER,
  PERSISTED_CACHE_KEY,
  PERSISTED_CACHE_MAX_AGE_MS,
  PERSISTED_DEHYDRATE_OPTIONS,
  purgePersistedCache,
} from './persistence'

function memoryStorage() {
  const items = new Map<string, string>()
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  }
}

const options = { buster: PERSISTED_CACHE_BUSTER, maxAge: PERSISTED_CACHE_MAX_AGE_MS }

async function saveOverview(storage: ReturnType<typeof memoryStorage>) {
  const queryClient = new QueryClient()
  queryClient.setQueryData(['overview'], { net_worth: 120_000 })
  const persister = createCachePersister(storage)
  await persistQueryClientSave({ queryClient, persister, buster: PERSISTED_CACHE_BUSTER })
  await vi.advanceTimersByTimeAsync(2_000) // écritures regroupées par le persister
  return persister
}

describe('persistance du cache pour le hors ligne', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('restaure les dernières données consultées', async () => {
    const storage = memoryStorage()
    const persister = await saveOverview(storage)
    expect(storage.items.has(PERSISTED_CACHE_KEY)).toBe(true)

    const restored = new QueryClient()
    await persistQueryClientRestore({ queryClient: restored, persister, ...options })
    expect(restored.getQueryData(['overview'])).toEqual({ net_worth: 120_000 })
  })

  it('n’a plus rien à restaurer une fois purgé (déconnexion)', async () => {
    const storage = memoryStorage()
    const persister = await saveOverview(storage)

    purgePersistedCache(storage)
    expect(storage.items.has(PERSISTED_CACHE_KEY)).toBe(false)

    const restored = new QueryClient()
    await persistQueryClientRestore({ queryClient: restored, persister, ...options })
    expect(restored.getQueryData(['overview'])).toBeUndefined()
  })

  it('ignore une copie plus ancienne que la durée maximale', async () => {
    const storage = memoryStorage()
    const persister = await saveOverview(storage)
    vi.setSystemTime(Date.now() + PERSISTED_CACHE_MAX_AGE_MS + 1)

    const restored = new QueryClient()
    await persistQueryClientRestore({ queryClient: restored, persister, ...options })
    expect(restored.getQueryData(['overview'])).toBeUndefined()
  })

  it('ne copie ni les mutations ni les requêtes marquées comme non persistées', () => {
    const queryClient = new QueryClient()
    queryClient.setQueryData(['overview'], { net_worth: 120_000 })
    const tokens = queryClient.getQueryCache().build(queryClient, { queryKey: ['access-tokens'], meta: { persist: false } })
    tokens.setData([{ id: 'jeton-1' }])
    // Une mutation en attente (hors ligne) garderait ses variables, mot de passe compris.
    queryClient.getMutationCache().build(queryClient, { mutationKey: ['create-token'] }).state.variables = {
      password: 'mot-de-passe-fictif',
    }

    const copy = dehydrate(queryClient, PERSISTED_DEHYDRATE_OPTIONS)
    expect(copy.queries.map((query) => query.queryKey)).toEqual([['overview']])
    expect(copy.mutations).toEqual([])
    expect(JSON.stringify(copy)).not.toContain('mot-de-passe-fictif')
  })

  it('supporte l’absence de stockage', () => {
    expect(() => purgePersistedCache(undefined)).not.toThrow()
  })
})

describe('date des données affichées hors ligne', () => {
  it('prend la plus ancienne des données à l’écran', () => {
    expect(
      offlineDataTimestamp([
        { dataUpdatedAt: 3_000, observed: true },
        { dataUpdatedAt: 1_000, observed: true },
        { dataUpdatedAt: 500, observed: false },
      ]),
    ).toBe(1_000)
  })
  it('à défaut, la plus récente du cache', () => {
    expect(
      offlineDataTimestamp([
        { dataUpdatedAt: 500, observed: false },
        { dataUpdatedAt: 2_000, observed: false },
      ]),
    ).toBe(2_000)
  })
  it('ignore les requêtes jamais chargées et renvoie null sans données', () => {
    expect(offlineDataTimestamp([{ dataUpdatedAt: 0, observed: true }])).toBeNull()
    expect(offlineDataTimestamp([])).toBeNull()
  })
})
