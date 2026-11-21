import { useQuery } from '@tanstack/react-query'
import { api } from '@/shared/api/endpoints'
import { useApiMutation } from '@/shared/api/queries'
import type { TransactionPatch } from '@/shared/api/types'
import { useView } from '@/shared/state/view'

export function useBudget(month: string) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['budget', month, memberId],
    queryFn: () => api.budget(month, { member_id: memberId }),
  })
}

export function useBudgetTransactions(month: string) {
  const { memberId } = useView()
  return useQuery({
    queryKey: ['budget-transactions', month, memberId],
    queryFn: () => api.budgetTransactions(month, { member_id: memberId }),
  })
}

export function useBudgetCategories() {
  return useQuery({ queryKey: ['budget-categories'], queryFn: api.budgetCategories })
}

export function useBudgetRules() {
  return useQuery({ queryKey: ['budget-rules'], queryFn: api.budgetRules })
}

export function useUpdateTransaction(onSuccess?: () => void) {
  return useApiMutation(({ id, patch }: { id: string; patch: TransactionPatch }) => api.updateTransaction(id, patch), onSuccess)
}

export function useSetBudgetTarget(onSuccess?: () => void) {
  return useApiMutation(
    (body: { category: string; monthly_amount: number | null }) => api.setBudgetTarget(body),
    onSuccess,
  )
}

export function useDeleteBudgetRule() {
  return useApiMutation((id: string) => api.deleteBudgetRule(id))
}
