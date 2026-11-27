import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { AccessTokenRevokeInput } from '@/shared/api/types'

const GRANTS_KEY = ['oauth-grants']

export function useOAuthGrants() {
  // Inutile hors ligne : jamais copié dans le stockage local.
  return useQuery({ queryKey: GRANTS_KEY, queryFn: api.oauthGrants, meta: { persist: false } })
}

export function useRevokeOAuthGrant(onSuccess: () => void) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: AccessTokenRevokeInput & { id: string }) => api.revokeOAuthGrant(id, input),
    // Le mot de passe envoyé ne reste pas dans le cache des mutations.
    gcTime: 0,
    onSuccess,
    // Aussi après un refus : « déjà révoquée » doit montrer l'état réel.
    onSettled: () => void queryClient.invalidateQueries({ queryKey: GRANTS_KEY }),
  })
}
