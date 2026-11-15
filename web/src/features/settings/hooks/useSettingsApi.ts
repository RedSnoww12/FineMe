import { useMutation } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { Bundle, ImportResult, MemberInput } from '@/shared/api/types'

/** Crée le membre, ou le met à jour quand `memberId` est fourni. */
export function useSaveMember(memberId: string | undefined, onSuccess: () => void) {
  return useApiMutation(
    (input: MemberInput) => (memberId ? api.updateMember(memberId, input) : api.createMember(input)),
    onSuccess,
  )
}

export function useDeleteMember(onSuccess: () => void) {
  return useApiMutation((id: string) => api.deleteMember(id), onSuccess)
}

export function useExportBundle(onSuccess: (bundle: Bundle) => void) {
  return useMutation({ mutationFn: api.exportBundle, onSuccess })
}

export function useImportBundle(onSuccess: (result: ImportResult) => void) {
  return useApiMutation((bundle: Bundle) => api.importBundle(bundle), onSuccess)
}
