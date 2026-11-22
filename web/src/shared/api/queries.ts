import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { useView } from '@/shared/state/view'
import { api } from '@/shared/api/endpoints'
import type { AccountDetail, AccountInput } from '@/shared/api/types'

export function useOverview() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['overview', memberId],
    queryFn: () => api.overview({ member_id: memberId }),
  })
}

export function useAccounts() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['accounts', memberId],
    queryFn: () => api.accounts({ member_id: memberId }),
  })
}

export function useAccount(id: string | undefined) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['account', id, memberId],
    queryFn: () => api.account(id as string, { member_id: memberId }),
    enabled: Boolean(id),
  })
}

export function useMembers() {
  return useQuery({ queryKey: ['members'], queryFn: api.members })
}

export function useGoals() {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['goals', memberId],
    queryFn: () => api.goals({ member_id: memberId }),
  })
}

/**
 * Mutation qui rafraîchit toutes les lectures après succès : presque chaque écriture
 * modifie les totaux affichés ailleurs (vue d'ensemble, objectifs, immobilier).
 */
export function useApiMutation<TVars, TData>(
  mutationFn: (vars: TVars) => Promise<TData>,
  onSuccess?: (data: TData, vars: TVars) => void,
) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn,
    onSuccess: (data, vars) => {
      void queryClient.invalidateQueries()
      onSuccess?.(data, vars)
    },
  })
}

/** Crée le compte, ou le met à jour quand `accountId` est fourni. */
export function useSaveAccount(accountId: string | undefined, onSuccess: (account: AccountDetail) => void) {
  return useApiMutation(
    (input: AccountInput) => (accountId ? api.updateAccount(accountId, input) : api.createAccount(input)),
    onSuccess,
  )
}

export function useDeleteAccount(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteAccount(id), onSuccess)
}

/** Tout ce dont le formulaire de compte a besoin : données de référence et enregistrement. */
export function useAccountEditor(accountId: string | undefined, onSaved: () => void) {
  const members = useMembers()
  const detail = useAccount(accountId)
  const accounts = useAccounts()
  const save = useSaveAccount(accountId, onSaved)
  return {
    loading: members.isPending || (Boolean(accountId) && detail.isPending),
    loadError: members.error ?? (accountId ? detail.error : null),
    members: members.data ?? [],
    detail: detail.data,
    accounts: accounts.data ?? [],
    onSubmit: save.mutate,
    saving: save.isPending,
    saveError: save.error,
  }
}

/** Détail (positions comprises) de tous les comptes pouvant porter des positions. Clé partagée avec la fiche d'un compte. */
export function useAllAccountDetails() {
  const { memberId } = useView()
  const accounts = useAccounts()
  const ids = (accounts.data ?? []).filter((a) => a.kind !== 'real_estate' && a.kind !== 'loan').map((a) => a.id)
  const details = useQueries({
    queries: ids.map((id) => ({
      queryKey: ['account', id, memberId],
      queryFn: () => api.account(id, { member_id: memberId }),
    })),
    combine: (results) => ({
      data: results.every((r) => r.data) ? results.flatMap((r) => (r.data ? [r.data] : [])) : undefined,
      isPending: results.some((r) => r.isPending),
      error: results.find((r) => r.error)?.error ?? null,
      refetch: () => results.forEach((r) => void r.refetch()),
    }),
  })
  return {
    data: accounts.data ? details.data : undefined,
    isPending: accounts.isPending || details.isPending,
    error: accounts.error ?? details.error,
    refetch: () => {
      void accounts.refetch()
      details.refetch()
    },
  }
}
