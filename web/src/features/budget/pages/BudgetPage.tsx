import { Link, useSearchParams } from 'react-router'
import { money, pctFromFraction, todayIso } from '@/shared/lib/format'
import { EmptyState, ErrorState, IconButton, LoadingBlock, PageHeader, Panel, Stat } from '@/shared/ui/ui'
import { CategoriesPanel } from '../components/CategoriesPanel'
import { RecurringPanel, RulesPanel } from '../components/RecurringPanel'
import { TransactionsPanel } from '../components/TransactionsPanel'
import { TrendPanel } from '../components/TrendPanel'
import { useBudget, useBudgetCategories, useBudgetRules, useBudgetTransactions } from '../hooks/useBudgetApi'
import { isEmptyMonth, isMonth, monthLabel, shiftMonth } from '../lib/budget'

function useMonth(): [string, (month: string) => void] {
  const [params, setParams] = useSearchParams()
  const raw = params.get('mois') ?? ''
  const month = isMonth(raw) ? raw : todayIso().slice(0, 7)
  return [month, (next) => setParams({ mois: next }, { replace: true })]
}

function MonthPicker({ month, onChange }: { month: string; onChange: (month: string) => void }) {
  return (
    <div className="flex items-center gap-1">
      <IconButton icon="back" label="Mois précédent" onClick={() => onChange(shiftMonth(month, -1))} />
      <label className="flex items-center">
        <span className="sr-only">Mois affiché</span>
        <input
          type="month"
          className="input num w-auto"
          value={month}
          onChange={(event) => {
            if (isMonth(event.target.value)) onChange(event.target.value)
          }}
        />
      </label>
      <IconButton icon="chevron" label="Mois suivant" onClick={() => onChange(shiftMonth(month, 1))} />
    </div>
  )
}

export default function BudgetPage() {
  const [month, setMonth] = useMonth()
  const budget = useBudget(month)
  const transactions = useBudgetTransactions(month)
  const categories = useBudgetCategories()
  const rules = useBudgetRules()

  const header = (
    <PageHeader
      title="Budget"
      description="Revenus, dépenses et épargne du mois, hors virements entre vos comptes."
      actions={<MonthPicker month={month} onChange={setMonth} />}
    />
  )

  if (budget.isPending || transactions.isPending) {
    return (
      <>
        {header}
        <LoadingBlock label="Chargement du budget…" />
      </>
    )
  }
  if (budget.isError || transactions.isError) {
    return (
      <>
        {header}
        <ErrorState
          error={budget.error ?? transactions.error}
          onRetry={() => {
            void budget.refetch()
            void transactions.refetch()
          }}
        />
      </>
    )
  }

  const data = budget.data
  const items = transactions.data
  const empty = items.length === 0 && isEmptyMonth(data, data.categories)
  const hasHistory = data.trend.some((row) => row.income > 0 || row.expenses > 0)

  return (
    <>
      {header}
      <datalist id="budget-categories">
        {(categories.data ?? []).map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
      <div className="flex flex-col gap-5">
        {empty ? (
          <EmptyState
            title={`Aucune transaction en ${monthLabel(month)}`}
            action={
              <Link className="font-medium text-accent underline" to="/comptes">
                Importer des transactions depuis un compte
              </Link>
            }
          >
            Le budget se calcule à partir des transactions importées (CSV ou connexion bancaire) de vos comptes.
          </EmptyState>
        ) : (
          <>
            <Panel>
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
                <Stat label="Revenus" value={money(data.income)} />
                <Stat label="Dépenses" value={money(data.expenses)} />
                <Stat label="Épargne" value={money(data.savings, { signed: true })} tone={data.savings < 0 ? 'neg' : 'pos'} />
                <Stat
                  label="Taux d’épargne"
                  value={pctFromFraction(data.savings_rate, 0)}
                  sub={data.savings_rate === null ? 'Aucun revenu ce mois-ci' : undefined}
                />
              </div>
              {data.internal_transfers > 0 ? (
                <p className="mt-3 text-[13.5px] text-muted">
                  {data.internal_transfers === 1
                    ? '1 virement interne exclu des calculs.'
                    : `${data.internal_transfers} virements internes exclus des calculs.`}
                </p>
              ) : null}
            </Panel>
            <div className="grid gap-5 lg:grid-cols-2">
              <CategoriesPanel categories={data.categories} expenses={data.expenses} />
              <RecurringPanel charges={data.recurring} />
            </div>
          </>
        )}
        {hasHistory ? <TrendPanel trend={data.trend} /> : null}
        {!empty ? <TransactionsPanel items={items} /> : null}
        <RulesPanel rules={rules.data ?? []} />
      </div>
    </>
  )
}
