import { useState } from 'react'
import { Link } from 'react-router'
import { useMembers } from '@/shared/api/queries'
import type { HistoryPoint } from '@/shared/api/types'
import { date as formatDate, money, todayIso } from '@/shared/lib/format'
import { useView } from '@/shared/state/view'
import { Button, EmptyState, ErrorState, LoadingBlock, PageHeader, Panel, Signed, TextField } from '@/shared/ui/ui'
import { useMemberHistories, useViewHistory } from '../hooks/useAnalysisApi'
import { compareSnapshots, quickDates, snapshotAt, type DeltaRow } from '../lib/timeTravel'

function DeltaTable({ caption, rows, compare }: { caption: string; rows: DeltaRow[]; compare: boolean }) {
  return (
    <div className="table-wrap">
      <table className="table">
        <caption className="sr-only">{caption}</caption>
        <thead>
          <tr>
            <th scope="col">Libellé</th>
            <th scope="col" className="r">{compare ? 'Date 1' : 'Valeur'}</th>
            {compare ? (
              <>
                <th scope="col" className="r">Date 2</th>
                <th scope="col" className="r">Écart</th>
              </>
            ) : null}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key}>
              <th scope="row" className="font-normal">{row.label}</th>
              <td className="r num">{money(row.from)}</td>
              {compare ? (
                <>
                  <td className="r num">{money(row.to)}</td>
                  <td className="r num">
                    <Signed value={row.delta}>{money(row.delta, { signed: true })}</Signed>
                  </td>
                </>
              ) : null}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** Écart du patrimoine net de chaque membre entre les deux dates (vue foyer seulement). */
function MembersPanel({ from, to }: { from: string; to: string }) {
  const members = useMembers()
  const list = members.data ?? []
  const histories = useMemberHistories(list.map((member) => member.id))
  if (list.length < 2) return null
  let content
  if (histories.isPending) content = <LoadingBlock rows={2} />
  else if (histories.error || !histories.data) content = <ErrorState error={histories.error} onRetry={histories.refetch} />
  else {
    const rows = list.map((member, i) => {
      const points = histories.data?.[i]?.points ?? []
      const net = (day: string) => snapshotAt(points, day)?.net_worth ?? 0
      return { key: member.id, label: member.name, from: net(from), to: net(to), delta: net(to) - net(from) }
    })
    content = <DeltaTable caption="Patrimoine net par membre" rows={rows} compare />
  }
  return <Panel title="Par membre">{content}</Panel>
}

function snapshotNote(requested: string, snapshot: HistoryPoint) {
  return snapshot.date === requested ? '' : ` (dernier instantané : ${formatDate(snapshot.date, 'long')})`
}

export default function TimeTravelPage() {
  const { memberId } = useView()
  const history = useViewHistory('all')
  const today = todayIso()
  const shortcuts = quickDates(today)
  const [from, setFrom] = useState(shortcuts[0]?.date ?? today)
  const [to, setTo] = useState(today)
  const [compare, setCompare] = useState(false)

  const header = (
    <PageHeader
      title="Patrimoine à une date"
      description="Revoyez votre patrimoine tel qu’il était à une date passée, ou comparez deux dates."
      back={
        <Link to="/analyse" className="mb-1 inline-flex text-[13.5px] text-muted hover:text-ink">
          ← Analyse
        </Link>
      }
    />
  )
  if (history.isPending) return <>{header}<LoadingBlock rows={4} /></>
  if (history.isError) return <>{header}<ErrorState error={history.error} onRetry={() => void history.refetch()} /></>

  const points = history.data.points
  const first = snapshotAt(points, from)
  const second = compare ? snapshotAt(points, to) : first
  const firstDate = points.reduce<string | null>((min, p) => (min === null || p.date < min ? p.date : min), null)

  return (
    <>
      {header}
      <section aria-label="Dates consultées" className="panel mb-4 flex flex-wrap items-end gap-x-4 gap-y-3 px-4 py-3 sm:mb-5 sm:px-5">
        <TextField label={compare ? 'Date 1' : 'Date'} type="date" value={from} max={today} onChange={setFrom} fieldClassName="w-full sm:w-44" />
        {compare ? <TextField label="Date 2" type="date" value={to} max={today} onChange={setTo} fieldClassName="w-full sm:w-44" /> : null}
        <div role="group" aria-label="Raccourcis de date" className="flex flex-wrap gap-2">
          {shortcuts.map((shortcut) => (
            <Button key={shortcut.label} size="sm" variant={from === shortcut.date ? 'primary' : 'secondary'} onClick={() => setFrom(shortcut.date)}>
              {shortcut.label}
            </Button>
          ))}
        </div>
        <label className="flex min-h-10 items-center gap-2 sm:ml-auto">
          <input type="checkbox" className="size-4 accent-[var(--accent)]" checked={compare} onChange={(e) => setCompare(e.target.checked)} />
          Comparer avec une autre date
        </label>
      </section>

      {first === null || second === null ? (
        <EmptyState title="Aucun instantané à cette date">
          {firstDate
            ? `L’historique commence le ${formatDate(firstDate, 'long')}. Choisissez une date à partir de celle-ci.`
            : 'FineMe enregistre chaque jour un instantané de votre patrimoine : revenez dans quelques jours.'}
        </EmptyState>
      ) : (
        <View first={first} second={second} from={from} to={to} compare={compare} showMembers={compare && memberId === null} />
      )}
    </>
  )
}

function View({
  first,
  second,
  from,
  to,
  compare,
  showMembers,
}: {
  first: HistoryPoint
  second: HistoryPoint
  from: string
  to: string
  compare: boolean
  showMembers: boolean
}) {
  const comparison = compareSnapshots(first, second)
  const banner = compare
    ? `Vous comparez le ${formatDate(from, 'long')}${snapshotNote(from, first)} au ${formatDate(to, 'long')}${snapshotNote(to, second)}.`
    : `Vous consultez le patrimoine au ${formatDate(from, 'long')}${snapshotNote(from, first)}, pas celui d’aujourd’hui.`
  return (
    <div className="flex flex-col gap-4 sm:gap-5">
      <p role="status" className="rounded-[11px] border border-brass/50 bg-surface-2 px-4 py-3 font-medium text-brass">
        {banner}
      </p>
      <Panel title="Totaux">
        <DeltaTable caption="Totaux du patrimoine" rows={comparison.totals} compare={compare} />
      </Panel>
      {comparison.categories && comparison.accounts ? (
        <div className="grid gap-4 sm:gap-5 lg:grid-cols-2">
          <Panel title="Par catégorie">
            <DeltaTable caption="Actifs par catégorie" rows={comparison.categories} compare={compare} />
          </Panel>
          <Panel title="Par compte">
            <DeltaTable caption="Valeur par compte (dettes en négatif)" rows={comparison.accounts} compare={compare} />
          </Panel>
        </div>
      ) : (
        <p className="text-muted">
          Le détail par catégorie et par compte n’existe pas pour cet instantané (relevé ancien ou corrigé à la main).
        </p>
      )}
      {showMembers ? <MembersPanel from={from} to={to} /> : null}
    </div>
  )
}
