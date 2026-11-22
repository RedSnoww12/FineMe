import { useState } from 'react'
import { Link } from 'react-router'
import { useAccounts, useDeleteAccount } from '@/shared/api/queries'
import type { Account, AccountKind } from '@/shared/api/types'
import { AccountEditor } from '../components/AccountEditor'
import { Icon } from '@/shared/ui/Icon'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  IconButton,
  LoadingBlock,
  PageHeader,
  Signed,
} from '@/shared/ui/ui'
import { money, pctFromFraction } from '@/shared/lib/format'
import { ACCOUNT_KIND_ORDER, ACCOUNT_KIND_PLURAL, envelopeLabel } from '@/shared/lib/labels'

const ALL_KINDS: AccountKind[] = ['checking', 'savings', 'investment', 'crypto', 'real_estate', 'loan', 'other']

function ownersText(account: Account): string {
  if (account.owners.length === 0) return 'Aucun propriétaire'
  return account.owners.map((owner) => `${owner.name} ${pctFromFraction(owner.share, 0)}`).join(', ')
}

function AccountRow({
  account,
  onEdit,
  onDelete,
}: {
  account: Account
  onEdit: () => void
  onDelete: () => void
}) {
  const partial = Math.abs(account.owned_value - account.value) > 0.005
  return (
    <li className="flex items-center gap-2 border-b border-line py-3 pr-2 pl-4 last:border-0 sm:pl-5">
      <Link to={`/comptes/${account.id}`} className="group flex min-w-0 flex-1 items-center gap-3 rounded-lg">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="truncate font-medium group-hover:underline">{account.name}</span>
            {account.envelope ? <Badge>{envelopeLabel(account.envelope)}</Badge> : null}
          </div>
          <div className="mt-0.5 truncate text-[13px] text-muted">
            {account.institution ? `${account.institution}, ` : ''}
            {ownersText(account)}
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className="num font-display text-[17px] font-semibold">{money(account.owned_value)}</div>
          <div className="num text-[13px] text-muted">
            {partial ? (
              <>sur {money(account.value)}</>
            ) : account.gain !== null ? (
              <Signed value={account.gain}>{money(account.gain, { signed: true })}</Signed>
            ) : account.is_liability ? (
              'restant dû'
            ) : null}
          </div>
        </div>
        <Icon name="chevron" size={16} className="shrink-0 text-muted max-sm:hidden" />
      </Link>
      <div className="flex shrink-0 max-sm:flex-col">
        <IconButton icon="edit" label={`Modifier ${account.name}`} onClick={onEdit} />
        <IconButton icon="trash" label={`Supprimer ${account.name}`} onClick={onDelete} />
      </div>
    </li>
  )
}

export default function AccountsPage() {
  const accounts = useAccounts()
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Account | null>(null)
  const [deleting, setDeleting] = useState<Account | null>(null)
  const remove = useDeleteAccount(() => setDeleting(null))

  const addButton = (
    <Button variant="primary" icon="plus" onClick={() => setCreating(true)}>
      Ajouter un compte
    </Button>
  )

  const groups = ACCOUNT_KIND_ORDER.map((kind) => ({
    kind,
    items: (accounts.data ?? []).filter((account) => account.kind === kind),
  })).filter((group) => group.items.length > 0)

  return (
    <>
      <PageHeader
        title="Comptes"
        description="Chaque compte appartient à un ou plusieurs membres du foyer, selon leurs parts."
        actions={
          accounts.isSuccess && accounts.data.length > 0 ? (
            <>
              <Link to="/comptes/positions" className="inline-flex min-h-10 items-center rounded-[9px] border border-line bg-surface px-4 font-medium">
                Positions consolidées
              </Link>
              {addButton}
            </>
          ) : undefined
        }
      />

      {accounts.isPending ? (
        <LoadingBlock rows={4} />
      ) : accounts.isError ? (
        <ErrorState error={accounts.error} onRetry={() => void accounts.refetch()} />
      ) : accounts.data.length === 0 ? (
        <EmptyState title="Aucun compte" action={addButton}>
          Commencez par un compte courant ou un livret : indiquez son solde et ses propriétaires. Pour un compte-titres
          ou un PEA, vous ajouterez ensuite les positions à la main ou par import CSV.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-4 sm:gap-5">
          {groups.map((group) => {
            const subtotal = group.items.reduce((sum, account) => sum + account.owned_value, 0)
            return (
              <section key={group.kind} className="panel">
                <header className="flex items-baseline justify-between gap-3 border-b border-line px-4 py-3 sm:px-5">
                  <h2 className="text-[16px] font-semibold">{ACCOUNT_KIND_PLURAL[group.kind]}</h2>
                  <span className="num text-muted">
                    {group.kind === 'loan' ? 'Restant dû : ' : ''}
                    {money(subtotal)}
                  </span>
                </header>
                <ul>
                  {group.items.map((account) => (
                    <AccountRow
                      key={account.id}
                      account={account}
                      onEdit={() => setEditing(account)}
                      onDelete={() => {
                        remove.reset()
                        setDeleting(account)
                      }}
                    />
                  ))}
                </ul>
              </section>
            )
          })}
        </div>
      )}

      {creating ? <AccountEditor kinds={ALL_KINDS} onClose={() => setCreating(false)} /> : null}
      {editing ? (
        <AccountEditor accountId={editing.id} kinds={[editing.kind]} onClose={() => setEditing(null)} />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer ce compte ?"
          confirmLabel="Supprimer le compte"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.name} », ses positions et ses opérations seront supprimés définitivement.
        </ConfirmModal>
      ) : null}
    </>
  )
}
