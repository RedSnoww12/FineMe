import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { AccessTokenInput, AccessTokenRevokeInput, CreatedAccessToken } from '@/shared/api/types'

const TOKENS_KEY = ['access-tokens']
const CALLS_KEY = ['mcp-calls']

export function useAccessTokens() {
  // Inutiles hors ligne : jamais copiés dans le stockage local.
  return useQuery({ queryKey: TOKENS_KEY, queryFn: api.accessTokens, meta: { persist: false } })
}

export function useMcpCalls() {
  return useQuery({ queryKey: CALLS_KEY, queryFn: () => api.mcpCalls(), meta: { persist: false } })
}

function useRefreshTokens() {
  const queryClient = useQueryClient()
  return () => void queryClient.invalidateQueries({ queryKey: TOKENS_KEY })
}

export function useCreateAccessToken(onSuccess: (created: CreatedAccessToken) => void) {
  const refresh = useRefreshTokens()
  return useMutation({
    mutationFn: (input: AccessTokenInput) => api.createAccessToken(input),
    // Ni le secret renvoyé ni le mot de passe envoyé ne restent dans le cache des mutations.
    gcTime: 0,
    onSuccess: (created) => {
      onSuccess(created)
      refresh()
    },
  })
}

export function useRevokeAccessToken(onSuccess: () => void) {
  const refresh = useRefreshTokens()
  return useMutation({
    mutationFn: ({ id, ...input }: AccessTokenRevokeInput & { id: string }) => api.revokeAccessToken(id, input),
    gcTime: 0,
    onSuccess: () => {
      onSuccess()
      refresh()
    },
  })
}
