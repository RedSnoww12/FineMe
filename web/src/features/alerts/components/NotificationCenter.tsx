import { useState } from 'react'
import { Link } from 'react-router'
import { useAlerts, useEvaluateAlerts, useReadAllAlerts, useUpdateAlert } from '../hooks/useAlertsApi'
import { alertMessage, badgeText, sortForDisplay, unreadCount, unreadLabel } from '../lib/alerts'
import type { Alert } from '@/shared/api/types'
import { dateTime } from '@/shared/lib/format'
import { useDiscreet } from '@/shared/state/discreet'
import { Icon } from '@/shared/ui/Icon'
import { Badge, Button, EmptyState, ErrorState, FormError, LoadingBlock, Modal, cx } from '@/shared/ui/ui'

function useUnreadCount(): number {
  const alerts = useAlerts()
  return alerts.data ? unreadCount(alerts.data) : 0
}

/** Annonce le nombre de notifications non lues aux lecteurs d'écran quand il change (une seule fois dans le shell). */
export function UnreadAnnouncer() {
  const alerts = useAlerts()
  return (
    <span role="status" aria-live="polite" className="sr-only">
      {alerts.data ? unreadLabel(unreadCount(alerts.data)) : ''}
    </span>
  )
}

function AlertItem({ alert }: { alert: Alert }) {
  const { hidden } = useDiscreet()
  const update = useUpdateAlert()
  return (
    <li className={cx('flex flex-col gap-2 py-3', alert.read && 'opacity-75')}>
      <div className="flex items-start gap-2.5">
        <span
          className={cx('mt-0.5 shrink-0', alert.severity === 'warning' ? 'text-brass' : 'text-accent')}
          aria-hidden="true"
        >
          <Icon name={alert.severity === 'warning' ? 'alert' : 'bell'} size={18} />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className={cx(alert.read ? 'font-medium' : 'font-semibold')}>
            {alert.read ? null : <span className="sr-only">Non lue : </span>}
            {alert.title}
          </span>
          <span className="text-[14px] text-muted">{alertMessage(alert, hidden)}</span>
          <span className="flex flex-wrap items-center gap-2 text-[13px] text-muted">
            <time dateTime={alert.created_at}>{dateTime(alert.created_at)}</time>
            {alert.active ? null : <Badge tone="pos">Résolue</Badge>}
          </span>
        </div>
      </div>
      <div className="flex flex-wrap gap-2 pl-7">
        <Button
          size="sm"
          variant="ghost"
          icon="check"
          busy={update.isPending && update.variables.read !== undefined}
          onClick={() => update.mutate({ id: alert.id, read: !alert.read })}
          aria-label={`${alert.read ? 'Marquer comme non lue' : 'Marquer comme lue'} : ${alert.title}`}
        >
          {alert.read ? 'Marquer comme non lue' : 'Marquer comme lue'}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          busy={update.isPending && update.variables.archived !== undefined}
          onClick={() => update.mutate({ id: alert.id, archived: true, read: true })}
          aria-label={`Archiver : ${alert.title}`}
        >
          Archiver
        </Button>
      </div>
      <FormError error={update.error} />
    </li>
  )
}

function NotificationsModal({ onClose }: { onClose: () => void }) {
  const alerts = useAlerts()
  const readAll = useReadAllAlerts()
  const evaluate = useEvaluateAlerts()
  const count = alerts.data ? unreadCount(alerts.data) : 0

  return (
    <Modal
      title="Notifications"
      onClose={onClose}
      footer={
        <>
          <Link
            to="/parametres#alertes"
            onClick={onClose}
            className="mr-auto inline-flex min-h-9 items-center text-[14px] font-medium text-accent hover:underline"
          >
            Régler les alertes
          </Link>
          <Button size="sm" icon="refresh" busy={evaluate.isPending} onClick={() => evaluate.mutate()}>
            Vérifier maintenant
          </Button>
          <Button size="sm" variant="primary" disabled={count === 0} busy={readAll.isPending} onClick={() => readAll.mutate()}>
            Tout marquer comme lu
          </Button>
        </>
      }
    >
      <FormError error={evaluate.error ?? readAll.error} />
      {alerts.isPending ? (
        <LoadingBlock rows={2} label="Chargement des notifications…" />
      ) : alerts.isError ? (
        <ErrorState error={alerts.error} onRetry={() => void alerts.refetch()} />
      ) : alerts.data.length === 0 ? (
        <EmptyState title="Aucune notification">
          Les alertes (objectif atteint, plafond de livret, variation du patrimoine, connexion ou cours à vérifier)
          apparaîtront ici.
        </EmptyState>
      ) : (
        <>
          <p className="text-muted">{unreadLabel(count)}.</p>
          <ul aria-label="Notifications" className="flex flex-col divide-y divide-line">
            {sortForDisplay(alerts.data).map((alert) => (
              <AlertItem key={alert.id} alert={alert} />
            ))}
          </ul>
        </>
      )}
    </Modal>
  )
}

/** Cloche du shell : badge du nombre de notifications non lues, ouvre le centre de notifications. */
export function NotificationCenter() {
  const [open, setOpen] = useState(false)
  const count = useUnreadCount()
  return (
    <>
      <button
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Notifications (${unreadLabel(count).toLowerCase()})`}
        title="Notifications"
        onClick={() => setOpen(true)}
        className="relative inline-flex size-10 shrink-0 items-center justify-center rounded-[9px] text-muted transition hover:bg-surface-2 hover:text-ink"
      >
        <Icon name="bell" size={20} />
        {count > 0 ? (
          <span
            aria-hidden="true"
            className="num absolute top-1 right-1 inline-flex min-w-[18px] items-center justify-center rounded-full bg-accent px-1 text-[11px] leading-[18px] font-semibold text-accent-ink"
          >
            {badgeText(count)}
          </span>
        ) : null}
      </button>
      {open ? <NotificationsModal onClose={() => setOpen(false)} /> : null}
    </>
  )
}
