import { useMemo, useState } from 'react'
import { Link } from 'react-router'
import { useAccounts, useGoals, useMembers, useOverview } from '@/shared/api/queries'
import { todayIso } from '@/shared/lib/format'
import { fillMissingDays } from '@/shared/lib/history'
import { useView } from '@/shared/state/view'
import { EmptyState, ErrorState, LoadingBlock, PageHeader, Segmented, SelectField, TextField } from '@/shared/ui/ui'
import { BalancePanel, CategoryEvolutionPanel, WaterfallPanel } from '../components/HistoryPanels'
import { GoalsPanel, RealEstatePanel } from '../components/PlanPanels'
import { ReturnsPanel } from '../components/ReturnsPanel'
import { BreakdownPanel, PerformancePanel } from '../components/SnapshotPanels'
import { useHouseholdHistory, useViewHistory } from '../hooks/useAnalysisApi'
import { periodError, rangeToFetch, withinPeriod, type Period, type PresetRange, type Unit } from '../lib/period'

type PeriodChoice = PresetRange | 'custom'

const PRESETS: readonly PresetRange[] = ['1m', '3m', '1y', 'all']

const PERIODS: { value: PeriodChoice; label: string }[] = [
  { value: '1m', label: '1 mois' },
  { value: '3m', label: '3 mois' },
  { value: '1y', label: '1 an' },
  { value: 'all', label: 'Depuis le début' },
  { value: 'custom', label: 'Personnalisée' },
]

const UNITS: { value: Unit; label: string }[] = [
  { value: 'eur', label: 'En €' },
  { value: 'pct', label: 'En %' },
]

function monthsAgo(months: number): string {
  const today = new Date()
  const past = new Date(today.getFullYear(), today.getMonth() - months, today.getDate())
  return `${past.getFullYear()}-${String(past.getMonth() + 1).padStart(2, '0')}-${String(past.getDate()).padStart(2, '0')}`
}

/** Période et unité communes à tous les graphiques de la page. */
function Controls({
  period,
  onPeriod,
  unit,
  onUnit,
}: {
  period: Period
  onPeriod: (period: Period) => void
  unit: Unit
  onUnit: (unit: Unit) => void
}) {
  const choice: PeriodChoice = period.kind === 'preset' ? period.range : 'custom'
  const error = periodError(period)
  const choose = (value: string) => {
    const preset = PRESETS.find((range) => range === value)
    onPeriod(preset ? { kind: 'preset', range: preset } : { kind: 'custom', from: monthsAgo(3), to: todayIso() })
  }
  return (
    <section aria-label="Réglages des graphiques" className="panel mb-4 flex flex-wrap items-end gap-x-4 gap-y-3 px-4 py-3 sm:mb-5 sm:px-5">
      <SelectField label="Période" value={choice} onChange={choose} options={PERIODS} fieldClassName="w-full sm:w-48" />
      {period.kind === 'custom' ? (
        <>
          <TextField
            label="Du"
            type="date"
            value={period.from}
            max={period.to || undefined}
            onChange={(from) => onPeriod({ ...period, from })}
            error={error ?? undefined}
            fieldClassName="min-w-0 flex-1 sm:w-44 sm:flex-none"
          />
          <TextField
            label="Au"
            type="date"
            value={period.to}
            min={period.from || undefined}
            onChange={(to) => onPeriod({ ...period, to })}
            fieldClassName="min-w-0 flex-1 sm:w-44 sm:flex-none"
          />
        </>
      ) : null}
      <div className="flex flex-col gap-1.5 sm:ml-auto">
        <span className="text-[13.5px] font-medium" aria-hidden="true">
          Unité
        </span>
        <Segmented label="Unité des graphiques" value={unit} onChange={onUnit} options={UNITS} />
      </div>
    </section>
  )
}

export default function AnalysisPage() {
  const { memberId } = useView()
  const members = useMembers()
  const overview = useOverview()
  const accounts = useAccounts()
  const goals = useGoals()
  const [period, setPeriod] = useState<Period>({ kind: 'preset', range: '3m' })
  const [unit, setUnit] = useState<Unit>('eur')
  const range = rangeToFetch(period)
  const history = useViewHistory(range)
  const household = useHouseholdHistory(range)

  const series = useMemo(() => fillMissingDays(withinPeriod(history.data?.points ?? [], period)), [history.data, period])
  const householdSeries = useMemo(
    () => fillMissingDays(withinPeriod(household.data?.points ?? [], period)),
    [household.data, period],
  )
  const viewName = memberId ? members.data?.find((member) => member.id === memberId)?.name : null
  const description = viewName
    ? `Graphiques détaillés de la part détenue par ${viewName}.`
    : 'Graphiques détaillés du patrimoine du foyer, toutes parts confondues.'

  if (overview.isPending) {
    return (
      <>
        <PageHeader title="Analyse" description={description} />
        <LoadingBlock rows={4} />
      </>
    )
  }
  if (overview.isError) {
    return (
      <>
        <PageHeader title="Analyse" description={description} />
        <ErrorState error={overview.error} onRetry={() => void overview.refetch()} />
      </>
    )
  }
  if (overview.data.gross_assets === 0 && overview.data.liabilities === 0) {
    return (
      <>
        <PageHeader title="Analyse" description={description} />
        <EmptyState
          title="Rien à analyser pour l’instant"
          action={
            <Link to="/comptes" className="inline-flex min-h-10 items-center rounded-[9px] bg-accent px-4 font-medium text-accent-ink">
              Ajouter un compte
            </Link>
          }
        >
          Les graphiques se construisent à partir de vos comptes et de l’historique que FineMe enregistre chaque jour.
        </EmptyState>
      </>
    )
  }

  const historyState = {
    isPending: history.isPending,
    error: history.error,
    onRetry: () => void history.refetch(),
  }
  const accountsState = {
    accounts: accounts.data ?? [],
    isPending: accounts.isPending,
    error: accounts.error,
    onRetry: () => void accounts.refetch(),
  }

  return (
    <>
      <PageHeader title="Analyse" description={description} />
      <Controls period={period} onPeriod={setPeriod} unit={unit} onUnit={setUnit} />
      <div className="flex flex-col gap-4 sm:gap-5">
        <CategoryEvolutionPanel series={series} unit={unit} {...historyState} />
        <ReturnsPanel period={period} />
        <div className="grid gap-4 sm:gap-5 lg:grid-cols-2">
          <BalancePanel series={series} unit={unit} {...historyState} />
          <WaterfallPanel series={series} unit={unit} {...historyState} />
          <BreakdownPanel overview={overview.data} unit={unit} showMembers={memberId === null} />
          <PerformancePanel {...accountsState} memberId={memberId} unit={unit} />
          <RealEstatePanel {...accountsState} memberId={memberId} unit={unit} />
          <GoalsPanel
            goals={goals.data ?? []}
            memberId={memberId}
            householdSeries={householdSeries}
            unit={unit}
            isPending={goals.isPending || household.isPending}
            error={goals.error ?? household.error}
            onRetry={() => {
              void goals.refetch()
              void household.refetch()
            }}
          />
        </div>
      </div>
    </>
  )
}
