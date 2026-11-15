import { useState } from 'react'
import { Link } from 'react-router'
import {
  useConnections,
  useCreateConnection,
  useDeleteConnection,
  useProviders,
  useSyncConnection,
} from '../hooks/useConnectionsApi'
import type { Connection, SyncResult } from '@/shared/api/types'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  FormError,
  LoadingBlock,
  Notice,
  PageHeader,
  Panel,
} from '@/shared/ui/ui'
import { dateTime } from '@/shared/lib/format'

const STATUS: Record<Connection['status'], { label: string; tone: 'pos' | 'neg' | 'brass' }> = {
  active: { label: 'Active', tone: 'pos' },
  error: { label: 'En erreur', tone: 'neg' },
  expired: { label: 'Expirée', tone: 'brass' },
}

export default function ConnectionsPage() {
  const providers = useProviders()
  const connections = useConnections()
  const [deleting, setDeleting] = useState<Connection | null>(null)
  const [synced, setSynced] = useState<{ id: string; result: SyncResult } | null>(null)
  const [redirectError, setRedirectError] = useState('')

  const connect = useCreateConnection((created) => {
    // On ne suit qu'une adresse web : le fournisseur y fait valider l'accès par la banque.
    if (/^https?:\/\//i.test(created.redirect_url)) window.location.assign(created.redirect_url)
    else setRedirectError('Le fournisseur n’a pas renvoyé d’adresse de connexion valide.')
  })
  const sync = useSyncConnection((result, id) => setSynced({ id, result }))
  const remove = useDeleteConnection(() => setDeleting(null))

  const providerName = (id: string) => providers.data?.find((provider) => provider.id === id)?.name ?? id

  return (
    <>
      <PageHeader
        title="Connexions"
        description="La synchronisation automatique avec vos banques passe par un agrégateur bancaire."
      />

      <div className="flex flex-col gap-4 sm:gap-5">
        {providers.isPending ? (
          <LoadingBlock rows={2} />
        ) : providers.isError ? (
          <ErrorState error={providers.error} onRetry={() => void providers.refetch()} />
        ) : providers.data.length === 0 ? (
          <EmptyState
            title="Aucun agrégateur bancaire n’est configuré"
            action={
              <Link
                to="/comptes"
                className="inline-flex min-h-10 items-center rounded-[9px] bg-accent px-4 font-medium text-accent-ink"
              >
                Aller aux comptes
              </Link>
            }
          >
            FineMe ne peut pas encore récupérer vos comptes directement auprès de vos banques. En attendant, ajoutez vos
            comptes à la main, puis importez vos opérations et vos positions depuis un fichier CSV, dans la page de
            chaque compte.
          </EmptyState>
        ) : (
          <Panel title="Agrégateurs disponibles" flush>
            <ul>
              {providers.data.map((provider) => (
                <li
                  key={provider.id}
                  className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3 last:border-0 sm:px-5"
                >
                  <div>
                    <div className="font-medium">{provider.name}</div>
                    <div className="text-[13px] text-muted">
                      {provider.configured
                        ? 'Prêt à connecter une banque.'
                        : 'Non configuré sur le serveur : ses identifiants d’accès sont manquants.'}
                    </div>
                  </div>
                  <Button
                    variant="primary"
                    icon="link"
                    disabled={!provider.configured}
                    busy={connect.isPending && connect.variables === provider.id}
                    onClick={() => {
                      setRedirectError('')
                      connect.mutate(provider.id)
                    }}
                  >
                    Connecter une banque
                  </Button>
                </li>
              ))}
            </ul>
            {connect.isError || redirectError ? (
              <div className="px-4 pb-4 sm:px-5">
                <FormError error={redirectError || connect.error} />
              </div>
            ) : null}
          </Panel>
        )}

        {connections.isPending ? (
          providers.isPending ? null : <LoadingBlock rows={1} />
        ) : connections.isError ? (
          providers.isError ? null : <ErrorState error={connections.error} onRetry={() => void connections.refetch()} />
        ) : connections.data.length > 0 ? (
          <Panel title="Banques connectées" flush>
            <ul>
              {connections.data.map((connection) => {
                const status = STATUS[connection.status] ?? { label: connection.status, tone: 'brass' as const }
                return (
                  <li key={connection.id} className="border-b border-line px-4 py-3 last:border-0 sm:px-5">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{connection.institution ?? providerName(connection.provider)}</span>
                          <Badge tone={status.tone}>{status.label}</Badge>
                        </div>
                        <div className="num text-[13px] text-muted">
                          via {providerName(connection.provider)},{' '}
                          {connection.last_sync_at
                            ? `synchronisée le ${dateTime(connection.last_sync_at)}`
                            : 'jamais synchronisée'}
                        </div>
                      </div>
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          icon="refresh"
                          busy={sync.isPending && sync.variables === connection.id}
                          onClick={() => {
                            setSynced(null)
                            sync.mutate(connection.id)
                          }}
                        >
                          Synchroniser
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          icon="trash"
                          onClick={() => {
                            remove.reset()
                            setDeleting(connection)
                          }}
                        >
                          Supprimer
                        </Button>
                      </div>
                    </div>
                    <div className="mt-2 flex flex-col gap-2 empty:hidden">
                      {connection.error ? <FormError error={connection.error} /> : null}
                      {sync.isError && sync.variables === connection.id ? <FormError error={sync.error} /> : null}
                      {synced?.id === connection.id ? (
                        <Notice tone="pos">
                          Synchronisation terminée : {synced.result.accounts_updated} compte
                          {synced.result.accounts_updated > 1 ? 's' : ''} mis à jour,{' '}
                          {synced.result.transactions_inserted} opération
                          {synced.result.transactions_inserted > 1 ? 's' : ''} ajoutée
                          {synced.result.transactions_inserted > 1 ? 's' : ''}.
                        </Notice>
                      ) : null}
                    </div>
                  </li>
                )
              })}
            </ul>
          </Panel>
        ) : providers.isSuccess && providers.data.length > 0 ? (
          <p className="text-muted">Aucune banque connectée pour l’instant.</p>
        ) : null}
      </div>

      {deleting ? (
        <ConfirmModal
          title="Supprimer cette connexion ?"
          confirmLabel="Supprimer la connexion"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          La synchronisation avec « {deleting.institution ?? providerName(deleting.provider)} » s’arrêtera.
        </ConfirmModal>
      ) : null}
    </>
  )
}
