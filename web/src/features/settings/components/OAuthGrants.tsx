import { useState } from 'react'
import { useOAuthGrants, useRevokeOAuthGrant } from '../hooks/useOAuthGrants'
import { mcpEndpoint } from '../lib/accessTokens'
import { RevokeAccessForm } from './Confirmation'
import { apiBaseUrl } from '@/shared/api/client'
import type { OAuthGrant } from '@/shared/api/types'
import { dateTime } from '@/shared/lib/format'
import { Badge, Button, ErrorState, LoadingBlock } from '@/shared/ui/ui'

function RevokeGrantForm({
  grant,
  totpEnabled,
  onCancel,
}: {
  grant: OAuthGrant
  totpEnabled: boolean
  onCancel: () => void
}) {
  const revoke = useRevokeOAuthGrant(onCancel)
  return (
    <RevokeAccessForm
      label={`Révoquer la connexion ${grant.client_name}`}
      totpEnabled={totpEnabled}
      error={revoke.error}
      busy={revoke.isPending}
      onConfirm={(input) => revoke.mutate({ id: grant.id, ...input })}
      onCancel={onCancel}
    />
  )
}

/** Adresse à saisir dans claude.ai pour ajouter FineMe. */
function ConnectorAddress() {
  return (
    <p className="max-w-[64ch] text-[13.5px] text-muted">
      Pour connecter claude.ai (si le serveur a le connecteur activé), ajoutez un connecteur personnalisé avec
      l’adresse <span className="font-mono break-all">{mcpEndpoint(apiBaseUrl(), window.location.origin)}</span>.
    </p>
  )
}

/** Clients autorisés par OAuth, et leur révocation. */
export function OAuthGrants({ totpEnabled }: { totpEnabled: boolean }) {
  const grants = useOAuthGrants()
  const [revoking, setRevoking] = useState<string | null>(null)

  if (grants.isPending) return <LoadingBlock rows={2} label="Chargement des connexions OAuth…" />
  if (grants.isError) return <ErrorState error={grants.error} onRetry={() => void grants.refetch()} />
  return (
    <div className="flex flex-col gap-2">
      <ConnectorAddress />
      {grants.data.length === 0 ? (
        <p className="text-muted">Aucune connexion OAuth pour le moment.</p>
      ) : (
        <ul aria-label="Clients autorisés" className="flex flex-col divide-y divide-line">
          {grants.data.map((grant) => {
            const active = grant.revoked_at === null
            return (
              <li key={grant.id} className="py-2.5">
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
                  <div className="flex min-w-0 flex-col">
                    <span className="font-medium break-words">{grant.client_name}</span>
                    <span className="text-[13.5px] text-muted">
                      <span className="font-mono break-all">{grant.redirect_host}</span> · autorisé le{' '}
                      {dateTime(grant.created_at)} ·{' '}
                      {grant.last_used_at ? `utilisé le ${dateTime(grant.last_used_at)}` : 'jamais utilisé'}
                    </span>
                    {/* Seule identité garantie : le nom et l'adresse de retour sont choisis par le client. */}
                    <span className="font-mono text-[12.5px] break-all text-muted">{grant.client_id}</span>
                  </div>
                  <div className="flex items-center gap-2.5">
                    <Badge tone={active ? 'pos' : 'muted'}>{active ? 'Active' : 'Révoquée'}</Badge>
                    {active && revoking !== grant.id ? (
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => setRevoking(grant.id)}
                        aria-label={`Révoquer la connexion ${grant.client_name}`}
                      >
                        Révoquer
                      </Button>
                    ) : null}
                  </div>
                </div>
                {active && revoking === grant.id ? (
                  <RevokeGrantForm grant={grant} totpEnabled={totpEnabled} onCancel={() => setRevoking(null)} />
                ) : null}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
