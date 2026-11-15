import { useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { ConnectionCreated, SyncResult } from '@/shared/api/types'

export function useProviders() {
  return useQuery({ queryKey: ['providers'], queryFn: api.providers })
}

export function useConnections() {
  return useQuery({ queryKey: ['connections'], queryFn: api.connections })
}

export function useCreateConnection(onSuccess: (created: ConnectionCreated) => void) {
  return useApiMutation((provider: string) => api.createConnection(provider), onSuccess)
}

export function useSyncConnection(onSuccess: (result: SyncResult, id: string) => void) {
  return useApiMutation((id: string) => api.syncConnection(id), onSuccess)
}

export function useDeleteConnection(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteConnection(id), onSuccess)
}
