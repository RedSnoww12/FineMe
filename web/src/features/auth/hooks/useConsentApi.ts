import { useMutation, useQuery } from '@tanstack/react-query'
import { isExpectedRedirect } from '../lib/consent'
import { leaveTo } from '../lib/leave'
import { api } from '@/shared/api/endpoints'
import type { OAuthRedirect } from '@/shared/api/types'

// Un consentement ne se diffère pas : hors ligne, l'échec est immédiat plutôt que rejoué au retour du réseau.
const NOW_OR_NEVER = { networkMode: 'always' } as const

/** La demande d'autorisation. Une fois traitée elle n'existe plus : pas de nouvelle lecture spontanée. */
export function useConsentRequest(id: string, enabled: boolean) {
  return useQuery({
    ...NOW_OR_NEVER,
    queryKey: ['oauth-request', id],
    queryFn: () => api.oauthRequest(id),
    enabled,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    gcTime: 0,
    meta: { persist: false },
  })
}

function leaveFor(redirectHost: string) {
  return ({ redirect_to }: OAuthRedirect) => {
    if (!isExpectedRedirect(redirect_to, redirectHost)) {
      throw new Error('Adresse de retour inattendue : la connexion est interrompue.')
    }
    leaveTo(redirect_to)
  }
}

export function useApproveRequest(id: string, redirectHost: string) {
  const leave = leaveFor(redirectHost)
  return useMutation({
    ...NOW_OR_NEVER,
    mutationFn: (input: { password: string; code?: string }) => api.approveOAuthRequest(id, input).then(leave),
    // Le mot de passe envoyé ne reste pas dans le cache des mutations.
    gcTime: 0,
  })
}

export function useDenyRequest(id: string, redirectHost: string) {
  const leave = leaveFor(redirectHost)
  return useMutation({ ...NOW_OR_NEVER, mutationFn: () => api.denyOAuthRequest(id).then(leave) })
}
