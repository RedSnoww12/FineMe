import { useCallback, useState, type FormEvent } from 'react'
import { Link } from 'react-router'
import { useMembers } from '@/shared/api/queries'
import type { HistoryReviewPoint } from '@/shared/api/types'
import { date, money, pctFromFraction, toInput, todayIso } from '@/shared/lib/format'
import { useView } from '@/shared/state/view'
import { Icon } from '@/shared/ui/Icon'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Modal,
  Notice,
  NumberField,
  PageHeader,
  Panel,
  Segmented,
  Signed,
  TextField,
} from '@/shared/ui/ui'
import {
  useCorrectHistoryPoint,
  useDeleteHistoryRange,
  useHistoryReview,
  useRecomputeHistory,
  useResetHistory,
} from '../hooks/useHistoryCleanup'
import {
  countInRange,
  isCorrectable,
  isResetConfirmed,
  newestFirst,
  parseCorrection,
  rangeErrors,
  RESET_WORD,
  suspiciousPoints,
  type CorrectionErrors,
  type RangeErrors,
} from '../lib/historyCleanup'

const BackLink = () => (
  <Link to="/parametres" className="mb-2 inline-flex items-center gap-1 text-[14px] text-muted hover:text-ink">
    <Icon name="back" size={15} />
    Paramètres
  </Link>
)

function deletedMessage(days: number): string {
  if (days === 0) return 'Aucun point dans cette période : rien n’a été supprimé.'
  return days === 1 ? '1 jour supprimé de l’historique.' : `${days} jours supprimés de l’historique.`
}

/** Nom de la vue affichée, pour rappeler à quoi s'applique une correction. */
function useViewName(): string {
  const { memberId } = useView()
  const members = useMembers()
  if (!memberId) return 'Ensemble du foyer'
  return members.data?.find((member) => member.id === memberId)?.name ?? 'Membre'
}

function CorrectionModal({ point, viewName, onClose }: { point: HistoryReviewPoint; viewName: string; onClose: () => void }) {
  const [gross, setGross] = useState(toInput(point.gross_assets, 2))
  const [liabilities, setLiabilities] = useState(toInput(point.liabilities, 2))
  const [errors, setErrors] = useState<CorrectionErrors>({})
  const mutation = useCorrectHistoryPoint(onClose)
  const preview = parseCorrection(gross, liabilities).input

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const parsed = parseCorrection(gross, liabilities)
    setErrors(parsed.errors ?? {})
    if (parsed.input) mutation.mutate({ date: point.date, input: parsed.input })
  }

  return (
    <Modal
      title={`Corriger le point du ${date(point.date)}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="history-correction" busy={mutation.isPending}>
            Enregistrer la correction
          </Button>
        </>
      }
    >
      <form id="history-correction" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <p className="text-muted">
          La correction ne concerne que la vue « {viewName} ». Le détail par compte de ce jour est retiré : il ne
          correspondrait plus aux totaux saisis.
        </p>
        <NumberField label="Actifs bruts" value={gross} onChange={setGross} error={errors.gross_assets} suffix="€" />
        <NumberField label="Dettes" value={liabilities} onChange={setLiabilities} error={errors.liabilities} suffix="€" />
        <p className="num">
          Patrimoine net corrigé : <span className="font-semibold">{preview ? money(preview.gross_assets - preview.liabilities) : '–'}</span>
        </p>
        <FormError error={mutation.error} />
      </form>
    </Modal>
  )
}

type Filter = 'all' | 'suspicious'

function PointsPanel({ points, threshold, viewName }: { points: HistoryReviewPoint[]; threshold: number; viewName: string }) {
  const [filter, setFilter] = useState<Filter>('all')
  const [editing, setEditing] = useState<HistoryReviewPoint | null>(null)
  const [deleting, setDeleting] = useState<HistoryReviewPoint | null>(null)
  const [message, setMessage] = useState('')
  const remove = useDeleteHistoryRange((result) => {
    setDeleting(null)
    setMessage(deletedMessage(result.deleted_days))
  })
  // Stable : un rechargement de la liste ne doit pas déplacer le focus de la fenêtre de correction.
  const closeEditor = useCallback(() => setEditing(null), [])
  const today = todayIso()
  const suspicious = suspiciousPoints(points)
  const shown = newestFirst(filter === 'suspicious' ? suspicious : points)

  return (
    <Panel
      title="Points enregistrés"
      flush
      action={
        <Segmented<Filter>
          label="Points affichés"
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: `Tous (${points.length})` },
            { value: 'suspicious', label: `Suspects (${suspicious.length})` },
          ]}
        />
      }
    >
      <div className="flex flex-col gap-3 px-4 sm:px-5">
        <p className="text-muted">
          Un point est signalé quand le patrimoine net varie de plus de {pctFromFraction(threshold, 0)} depuis le point
          précédent (et d’au moins 100 €). Le pic est signalé, pas le retour à la normale qui le suit.
        </p>
        {message ? <Notice tone="pos">{message}</Notice> : null}
      </div>
      {shown.length === 0 ? (
        <p className="px-4 py-5 text-muted sm:px-5">Aucun point suspect : la courbe ne présente pas de variation anormale.</p>
      ) : (
        <div className="table-wrap mt-2">
          <table className="table">
            <thead>
              <tr>
                <th>Date</th>
                <th className="r">Patrimoine net</th>
                <th className="r">Actifs bruts</th>
                <th className="r">Dettes</th>
                <th className="r">Variation</th>
                <th>État</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((point) => (
                <tr key={point.date}>
                  <td className="num">{date(point.date)}</td>
                  <td className="r num font-medium">{money(point.net_worth)}</td>
                  <td className="r num">{money(point.gross_assets)}</td>
                  <td className="r num">{money(point.liabilities)}</td>
                  <td className="r">
                    <Signed value={point.change}>{pctFromFraction(point.change, 1, true)}</Signed>
                  </td>
                  <td>
                    <div className="flex flex-wrap gap-1.5">
                      {point.suspicious ? <Badge tone="neg">Suspect</Badge> : null}
                      {point.corrected ? <Badge tone="accent">Corrigé</Badge> : null}
                      {point.date === today ? <Badge>Aujourd’hui</Badge> : null}
                    </div>
                  </td>
                  <td>
                    <div className="flex justify-end">
                      <IconButton
                        icon="edit"
                        label={`Corriger le point du ${date(point.date)}`}
                        disabled={!isCorrectable(point, today)}
                        onClick={() => setEditing(point)}
                      />
                      <IconButton
                        icon="trash"
                        label={`Supprimer le point du ${date(point.date)}`}
                        onClick={() => {
                          remove.reset()
                          setDeleting(point)
                        }}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="px-4 py-3 text-[13px] text-muted sm:px-5">
        Le point du jour ne se corrige pas : il est recalculé à chaque instantané. Corrigez le compte concerné, puis
        recalculez-le ci-dessous.
      </p>

      {editing ? <CorrectionModal point={editing} viewName={viewName} onClose={closeEditor} /> : null}
      {deleting ? (
        <ConfirmModal
          title={`Supprimer le point du ${date(deleting.date)} ?`}
          confirmLabel="Supprimer le point"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate({ from: deleting.date, to: deleting.date })}
          onClose={() => setDeleting(null)}
        >
          Ce jour sera retiré de l’historique du foyer et de chaque membre. Les graphiques l’estimeront à partir des
          jours voisins.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}

function RangePanel({ points }: { points: HistoryReviewPoint[] }) {
  const [range, setRange] = useState({ from: '', to: '' })
  const [errors, setErrors] = useState<RangeErrors>({})
  const [confirming, setConfirming] = useState(false)
  const [message, setMessage] = useState('')
  const remove = useDeleteHistoryRange((result) => {
    setConfirming(false)
    setMessage(deletedMessage(result.deleted_days))
    setRange({ from: '', to: '' })
  })
  const set = (key: keyof typeof range) => (value: string) => {
    setMessage('')
    setRange((current) => ({ ...current, [key]: value }))
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next = rangeErrors(range.from, range.to)
    setErrors(next)
    if (Object.keys(next).length > 0) return
    remove.reset()
    setConfirming(true)
  }

  const count = countInRange(points, range.from, range.to)
  return (
    <Panel title="Supprimer une période">
      <form onSubmit={submit} noValidate className="flex flex-col gap-4">
        <p className="text-muted">
          Utile après une journée de données fausses ou des essais. Les jours sont supprimés pour le foyer et pour chaque
          membre.
        </p>
        <div className="grid gap-4 sm:grid-cols-2">
          <TextField label="Du" type="date" value={range.from} onChange={set('from')} error={errors.from} />
          <TextField label="Au (inclus)" type="date" value={range.to} onChange={set('to')} error={errors.to} />
        </div>
        {message ? <Notice tone="pos">{message}</Notice> : null}
        <div>
          <Button type="submit" variant="danger" icon="trash">
            Supprimer la période
          </Button>
        </div>
      </form>
      {confirming ? (
        <ConfirmModal
          title="Supprimer cette période ?"
          confirmLabel="Supprimer la période"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(range)}
          onClose={() => setConfirming(false)}
        >
          Du {date(range.from)} au {date(range.to)} : {count} point(s) de la vue affichée, supprimés pour le foyer et
          chaque membre. Cette action est définitive.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}

function RecomputePanel() {
  const [done, setDone] = useState<string | null>(null)
  const recompute = useRecomputeHistory((result) => setDone(result.snapshot_date))
  return (
    <Panel title="Recalculer le point du jour">
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="max-w-[60ch] text-muted">
            Reprend l’instantané d’aujourd’hui à partir des comptes et des cours actuels, pour le foyer et chaque membre.
            Les jours passés ne peuvent pas être recalculés : FineMe ne conserve pas l’historique des soldes et des
            valeurs des biens. Corrigez-les ou supprimez-les.
          </p>
          <Button icon="refresh" busy={recompute.isPending} onClick={() => recompute.mutate()}>
            Recalculer aujourd’hui
          </Button>
        </div>
        <FormError error={recompute.error} />
        {done ? <Notice tone="pos">Point du {date(done)} recalculé à partir des données actuelles.</Notice> : null}
      </div>
    </Panel>
  )
}

/** Fenêtre de confirmation forte : le mot saisi vit ici, pour que la fenêtre garde le focus pendant la frappe. */
function ResetModal({ onClose, onDone }: { onClose: () => void; onDone: (deletedDays: number) => void }) {
  const [word, setWord] = useState('')
  const reset = useResetHistory((result) => onDone(result.deleted_days))
  const confirmed = isResetConfirmed(word)

  return (
    <Modal
      title="Effacer tout l’historique ?"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="danger" type="submit" form="history-reset" busy={reset.isPending} disabled={!confirmed}>
            Effacer définitivement
          </Button>
        </>
      }
    >
      <form
        id="history-reset"
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          if (confirmed) reset.mutate(word)
        }}
      >
        <p className="text-muted">
          Tous les instantanés du foyer et des membres seront supprimés. Cette action est irréversible : pensez à
          exporter vos données avant.
        </p>
        <TextField
          label={`Saisissez ${RESET_WORD} pour confirmer`}
          value={word}
          onChange={setWord}
          autoComplete="off"
          spellCheck={false}
        />
        <FormError error={reset.error} />
      </form>
    </Modal>
  )
}

function ResetPanel() {
  const [open, setOpen] = useState(false)
  const [message, setMessage] = useState('')

  return (
    <Panel title="Effacer tout l’historique">
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="max-w-[60ch] text-muted">
            Supprime tous les points de tous les membres. Les graphiques repartiront de zéro au prochain instantané.
          </p>
          <Button
            variant="danger"
            icon="trash"
            onClick={() => {
              setMessage('')
              setOpen(true)
            }}
          >
            Effacer l’historique
          </Button>
        </div>
        {message ? <Notice tone="pos">{message}</Notice> : null}
      </div>
      {open ? (
        <ResetModal
          onClose={() => setOpen(false)}
          onDone={(days) => {
            setOpen(false)
            setMessage(deletedMessage(days))
          }}
        />
      ) : null}
    </Panel>
  )
}

export default function HistoryCleanupPage() {
  const review = useHistoryReview()
  const viewName = useViewName()

  return (
    <>
      <PageHeader
        title="Historique des graphiques"
        back={<BackLink />}
        description={`Vue « ${viewName} ». Corrigez ou supprimez les points aberrants : les graphiques se mettent à jour aussitôt.`}
      />
      <div className="flex flex-col gap-4 sm:gap-5">
        {review.isPending ? (
          <LoadingBlock rows={2} label="Chargement de l’historique" />
        ) : review.isError ? (
          <ErrorState error={review.error} onRetry={() => void review.refetch()} />
        ) : review.data.points.length === 0 ? (
          <EmptyState title="Aucun point d’historique">
            Un instantané est enregistré chaque jour (à l’ouverture de la vue d’ensemble ou par la tâche quotidienne). Il
            n’y a encore rien à nettoyer.
          </EmptyState>
        ) : (
          <>
            <PointsPanel points={review.data.points} threshold={review.data.threshold} viewName={viewName} />
            <RangePanel points={review.data.points} />
          </>
        )}
        <RecomputePanel />
        {review.data && review.data.points.length > 0 ? <ResetPanel /> : null}
      </div>
    </>
  )
}
