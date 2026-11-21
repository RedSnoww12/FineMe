import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { AuthResult, TotpActivation, TotpDisableInput } from '@/shared/api/types'

const STATUS_KEY = ['totp-status']

export function useTotpStatus() {
  return useQuery({ queryKey: STATUS_KEY, queryFn: api.totpStatus })
}

/** Après un changement, l'état du second facteur et le journal des connexions sont relus. */
function useRefreshTotp() {
  const queryClient = useQueryClient()
  return () => {
    void queryClient.invalidateQueries({ queryKey: STATUS_KEY })
    void queryClient.invalidateQueries({ queryKey: ['login-history'] })
  }
}

export function useStartTotp() {
  return useMutation({ mutationFn: api.totpSetup })
}

export function useEnableTotp(onSuccess: (activation: TotpActivation) => void) {
  const refresh = useRefreshTotp()
  return useMutation({
    mutationFn: api.totpEnable,
    onSuccess: (activation) => {
      onSuccess(activation)
      refresh()
    },
  })
}

export function useDisableTotp(onSuccess: (result: AuthResult) => void) {
  const refresh = useRefreshTotp()
  return useMutation({
    mutationFn: (input: TotpDisableInput) => api.totpDisable(input),
    onSuccess: (result) => {
      onSuccess(result)
      refresh()
    },
  })
}

export function useRegenerateRecoveryCodes(onSuccess: (codes: string[]) => void) {
  const refresh = useRefreshTotp()
  return useMutation({
    mutationFn: api.totpRecoveryCodes,
    onSuccess: (result) => {
      onSuccess(result.recovery_codes)
      refresh()
    },
  })
}
