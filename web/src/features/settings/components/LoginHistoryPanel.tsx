import { useLoginHistory } from '../hooks/useSettingsApi'
import { describeAttempt, describeUserAgent, failureCount } from '../lib/loginHistory'
import { dateTime } from '@/shared/lib/format'
import { Badge, ErrorState, LoadingBlock, Panel } from '@/shared/ui/ui'

/** Dernières tentatives de connexion et de réinitialisation (journal de l'API). */
export function LoginHistoryPanel() {
  const history = useLoginHistory()

  return (
    <Panel title="Connexions récentes">
      {history.isPending ? (
        <LoadingBlock rows={2} label="Chargement des connexions…" />
      ) : history.isError ? (
        <ErrorState error={history.error} onRetry={() => void history.refetch()} />
      ) : history.data.length === 0 ? (
        <p className="text-muted">Aucune connexion enregistrée pour le moment.</p>
      ) : (
        <div className="flex flex-col gap-3">
          <p className="text-muted">
            {history.data.length} dernières tentatives, dont {failureCount(history.data)} refusée
            {failureCount(history.data) > 1 ? 's' : ''}. Une adresse inconnue ? Changez votre mot de passe.
          </p>
          <ul aria-label="Connexions récentes" className="flex flex-col divide-y divide-line">
            {history.data.map((event, index) => (
              <li key={`${event.at}-${index}`} className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-2.5">
                <div className="flex min-w-0 flex-col">
                  <span className="font-medium">{describeAttempt(event)}</span>
                  <span className="truncate text-[13.5px] text-muted" title={event.user_agent ?? undefined}>
                    {describeUserAgent(event.user_agent)} · {event.ip ?? 'IP inconnue'}
                  </span>
                </div>
                <div className="flex items-center gap-2.5">
                  <time dateTime={event.at} className="text-[13.5px] text-muted">
                    {dateTime(event.at)}
                  </time>
                  <Badge tone={event.success ? 'pos' : 'neg'}>{event.success ? 'Réussie' : 'Refusée'}</Badge>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Panel>
  )
}
