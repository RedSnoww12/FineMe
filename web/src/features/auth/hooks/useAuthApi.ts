import { useMutation, useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import type { AuthResult, LoginResult } from '@/shared/api/types'

export function useAuthStatus() {
  return useQuery({ queryKey: ['auth-status'], queryFn: api.authStatus, staleTime: 0 })
}

export function useSetup(onSuccess: (result: AuthResult) => void) {
  return useMutation({ mutationFn: api.setup, onSuccess })
}

export function useLogin(onSuccess: (result: LoginResult) => void) {
  return useMutation({ mutationFn: api.login, onSuccess })
}

/** Seconde étape : le code TOTP (ou de secours) avec le jeton de défi. */
export function useLoginTotp(onSuccess: (result: AuthResult) => void) {
  return useMutation({ mutationFn: api.loginTotp, onSuccess })
}

export function useResetPassword(onSuccess: (result: AuthResult) => void) {
  return useMutation({ mutationFn: api.resetPassword, onSuccess })
}
