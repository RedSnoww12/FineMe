import { useState } from 'react'
import { Link } from 'react-router'
import type { SnapshotRef } from '@/shared/api/types'
import { date as formatDate, money, todayIso } from '@/shared/lib/format'
import { useView } from '@/shared/state/view'
import { Button, EmptyState, ErrorState, LoadingBlock, PageHeader, Panel, Signed, TextField } from '@/shared/ui/ui'
import { useHistoryComparison } from '../hooks/useAnalysisApi'
import { isIsoDate, labelComparison, quickDates, type DeltaRow, type LabeledComparison } from '../lib/timeTravel'

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

function snapshotNote(snapshot: SnapshotRef) {
  return snapshot.date === snapshot.requested ? '' : ` (dernier instantané : ${formatDate(snapshot.date, 'long')})`
}

export default function TimeTravelPage() {
  const { memberId } = useView()
  const today = todayIso()
  const shortcuts = quickDates(today)
  const [from, setFrom] = useState(shortcuts[0]?.date ?? today)
  const [to, setTo] = useState(today)
  const [compare, setCompare] = useState(false)
  const second = compare ? to : from
  const datesValid = isIsoDate(from) && isIsoDate(second)
  const comparison = useHistoryComparison(from, second, datesValid)

  // Le contenu dépend de la réponse ; la saisie des dates reste toujours affichée pour pouvoir corriger.
  let content
  if (!datesValid) content = <p className="text-muted">Indiquez une date complète (jour, mois et année).</p>
  else if (comparison.isPending) content = <LoadingBlock rows={4} />
  else if (comparison.isError)
    content = <ErrorState error={comparison.error} onRetry={() => void comparison.refetch()} />
  else {
    const result = comparison.data
    const labeled = labelComparison(result)
    content =
      labeled === null || result.a === null || result.b === null ? (
        <EmptyState title="Aucun instantané à cette date">
          {result.first_date
            ? `L’historique commence le ${formatDate(result.first_date, 'long')}. Choisissez une date à partir de celle-ci.`
            : 'FineMe enregistre chaque jour un instantané de votre patrimoine : revenez dans quelques jours.'}
        </EmptyState>
      ) : (
        <View
          labeled={labeled}
          first={result.a}
          second={result.b}
          compare={compare}
          showMembers={compare && memberId === null}
        />
      )
  }

  return (
    <>
      <PageHeader
        title="Patrimoine à une date"
        description="Revoyez votre patrimoine tel qu’il était à une date passée, ou comparez deux dates."
        back={
          <Link to="/analyse" className="mb-1 inline-flex text-[13.5px] text-muted hover:text-ink">
            ← Analyse
          </Link>
        }
      />
      <section
        aria-label="Dates consultées"
        className="panel mb-4 flex flex-wrap items-end gap-x-4 gap-y-3 px-4 py-3 sm:mb-5 sm:px-5"
      >
        <TextField
          label={compare ? 'Date 1' : 'Date'}
          type="date"
          value={from}
          max={today}
          onChange={setFrom}
          fieldClassName="w-full sm:w-44"
        />
        {compare ? (
          <TextField label="Date 2" type="date" value={to} max={today} onChange={setTo} fieldClassName="w-full sm:w-44" />
        ) : null}
        <div role="group" aria-label="Raccourcis de date" className="flex flex-wrap gap-2">
          {shortcuts.map((shortcut) => (
            <Button
              key={shortcut.label}
              size="sm"
              variant={from === shortcut.date ? 'primary' : 'secondary'}
              onClick={() => setFrom(shortcut.date)}
            >
              {shortcut.label}
            </Button>
          ))}
        </div>
        <label className="flex min-h-10 items-center gap-2 sm:ml-auto">
          <input
            type="checkbox"
            className="size-4 accent-[var(--accent)]"
            checked={compare}
            onChange={(e) => setCompare(e.target.checked)}
          />
          Comparer avec une autre date
        </label>
      </section>
      {content}
    </>
  )
}

function View({
  labeled,
  first,
  second,
  compare,
  showMembers,
}: {
  labeled: LabeledComparison
  first: SnapshotRef
  second: SnapshotRef
  compare: boolean
  showMembers: boolean
}) {
  const banner = compare
    ? `Vous comparez le ${formatDate(first.requested, 'long')}${snapshotNote(first)} au ${formatDate(second.requested, 'long')}${snapshotNote(second)}.`
    : `Vous consultez le patrimoine au ${formatDate(first.requested, 'long')}${snapshotNote(first)}, pas celui d’aujourd’hui.`
  const members = labeled.members ?? []
  return (
    <div className="flex flex-col gap-4 sm:gap-5">
      <p role="status" className="rounded-[11px] border border-brass/50 bg-surface-2 px-4 py-3 font-medium text-brass">
        {banner}
      </p>
      <Panel title="Totaux">
        <DeltaTable caption="Totaux du patrimoine" rows={labeled.totals} compare={compare} />
      </Panel>
      {labeled.categories && labeled.accounts ? (
        <div className="grid gap-4 sm:gap-5 lg:grid-cols-2">
          <Panel title="Par catégorie">
            <DeltaTable caption="Actifs par catégorie" rows={labeled.categories} compare={compare} />
          </Panel>
          <Panel title="Par compte">
            <DeltaTable caption="Valeur par compte (dettes en négatif)" rows={labeled.accounts} compare={compare} />
          </Panel>
        </div>
      ) : (
        <p className="text-muted">
          Le détail par catégorie et par compte n’existe pas pour cet instantané (relevé ancien ou corrigé à la main).
        </p>
      )}
      {showMembers && members.length >= 2 ? (
        <Panel title="Par membre">
          <DeltaTable caption="Patrimoine net par membre" rows={members} compare />
        </Panel>
      ) : null}
    </div>
  )
}
