import { useAccountEditor } from '@/shared/api/queries'
import type { AccountKind } from '@/shared/api/types'
import { AccountFormModal } from '@/shared/ui/AccountForm'

type Props = {
  accountId?: string
  kinds: AccountKind[]
  propertyAccountId?: string
  onClose: () => void
}

/** Relie le formulaire de compte partagé aux requêtes. */
export function AccountEditor(props: Props) {
  const editor = useAccountEditor(props.accountId, props.onClose)
  return <AccountFormModal {...props} {...editor} />
}
