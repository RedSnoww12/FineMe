import { useState } from 'react'
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { useAccounts, useGoals, useOverview } from '@/shared/api/queries'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import { money, moneyCompact, number, pct, toInput } from '@/shared/lib/format'
import { parseFrNumber } from '@/shared/lib/parse'
import { Button, EmptyState, ErrorState, LoadingBlock, NumberField, PageHeader, Panel, ProgressBar, Signed, Stat } from '@/shared/ui/ui'
import {
  applyScenario,
  defaultMonthlyExpenses,
  goalImpacts,
  NEUTRAL,
  PRESETS,
  type ScenarioParams,
  type ScenarioResult,
} from '../lib/scenario'

type Key = keyof ScenarioParams
type Form = Record<Key, string>

const FIELDS: { key: Key; label: string; suffix: string }[] = [
  { key: 'stocksPct', label: 'Variation des investissements', suffix: '%' },
  { key: 'cryptoPct', label: 'Variation des cryptos', suffix: '%' },
  { key: 'realEstatePct', label: 'Variation de l’immobilier', suffix: '%' },
  { key: 'incomeLossMonths', label: 'Mois sans revenus', suffix: 'mois' },
  { key: 'monthlyExpenses', label: 'Dépenses mensuelles', suffix: '€' },
  { key: 'purchasePrice', label: 'Prix d’un achat immobilier', suffix: '€' },
  { key: 'downPayment', label: 'Apport de l’achat', suffix: '€' },
  { key: 'inheritance', label: 'Héritage ou don reçu', suffix: '€' },
]

function toForm(params: Omit<ScenarioParams, 'monthlyExpenses'>, monthlyExpenses: string): Form {
  const form = Object.fromEntries(Object.entries(params).map(([key, value]) => [key, value === 0 ? '' : toInput(value)]))
  return { ...(form as Omit<Form, 'monthlyExpenses'>), monthlyExpenses }
}

function toParams(form: Form): ScenarioParams {
  return Object.fromEntries(Object.entries(form).map(([key, raw]) => [key, parseFrNumber(raw) ?? 0])) as ScenarioParams
}

function Delta({ before, after, format = money }: { before: number; after: number; format?: (value: number) => string }) {
  const change = after - before
  return <Signed value={change}>{change === 0 ? '–' : `${change > 0 ? '+' : ''}${format(change)}`}</Signed>
}

function Summary({ result }: { result: ScenarioResult }) {
  const { netWorth, coverageMonths, shortfall } = result
  return (
    <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
      <Stat label="Patrimoine net" value={money(netWorth.after)} sub={<Delta before={netWorth.before} after={netWorth.after} />} />
      <Stat label="Actifs bruts" value={money(result.grossAssets.after)} sub={<Delta {...result.grossAssets} />} />
      <Stat label="Dettes" value={money(result.liabilities.after)} sub={<Delta {...result.liabilities} />} />
      <Stat
        label="Mois de dépenses couverts"
        value={coverageMonths ? number(coverageMonths.after, 1) : '–'}
        sub={coverageMonths ? `avant : ${number(coverageMonths.before, 1)}` : 'Saisissez vos dépenses mensuelles'}
        tone={coverageMonths && coverageMonths.after < 3 ? 'neg' : undefined}
      />
      {shortfall > 0 ? (
        <p className="col-span-full text-[13.5px] text-neg">
          Vos liquidités ne suffisent pas : il manque {money(shortfall)} pour couvrir les dépenses et l’apport.
        </p>
      ) : null}
    </div>
  )
}

function Comparison({ result }: { result: ScenarioResult }) {
  const rows = result.categories.map((c) => ({ label: c.label, Avant: c.before, Après: c.after }))
  return (
    <>
      <div role="img" aria-label="Actifs par catégorie, avant et après le scénario." className="h-[260px]">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={rows} accessibilityLayer={false}>
            <CartesianGrid vertical={false} stroke="var(--line)" />
            <XAxis dataKey="label" tick={AXIS_TICK} tickLine={false} axisLine={false} interval={0} />
            <YAxis tick={AXIS_TICK} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => moneyCompact(v)} />
            <Tooltip content={<MoneyTooltip />} cursor={{ fill: 'var(--surface-2)' }} />
            <Bar dataKey="Avant" fill="var(--c7)" radius={[4, 4, 0, 0]} isAnimationActive={false} />
            <Bar dataKey="Après" fill="var(--c1)" radius={[4, 4, 0, 0]} isAnimationActive={false} />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <Legend items={[{ label: 'Avant', color: 'var(--c7)' }, { label: 'Après', color: 'var(--c1)' }]} />
      <div className="table-wrap mt-3">
        <table className="table">
          <caption className="sr-only">Actifs par catégorie, avant et après le scénario</caption>
          <thead>
            <tr>
              <th scope="col">Catégorie</th>
              <th scope="col" className="r">Avant</th>
              <th scope="col" className="r">Après</th>
              <th scope="col" className="r">Écart</th>
            </tr>
          </thead>
          <tbody>
            {result.categories.map((c) => (
              <tr key={c.category}>
                <th scope="row" className="font-normal">{c.label}</th>
                <td className="r num">{money(c.before)}</td>
                <td className="r num">{money(c.after)}</td>
                <td className="r num"><Delta before={c.before} after={c.after} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  )
}

export default function ScenariosPage() {
  const overview = useOverview()
  const goals = useGoals()
  const accounts = useAccounts()
  const suggestedExpenses = toInput(defaultMonthlyExpenses(goals.data ?? []) || null, 2)
  const [form, setForm] = useState<Form | null>(null)
  // Tant que rien n'est saisi, les dépenses suivent celles du matelas de sécurité, chargées après coup.
  const current = form ?? toForm(NEUTRAL, suggestedExpenses)

  const description = 'Testez un événement sur votre patrimoine réel. Rien n’est enregistré : vos données ne changent pas.'
  if (overview.isPending) {
    return (
      <>
        <PageHeader title="Et si ?" description={description} />
        <LoadingBlock rows={4} />
      </>
    )
  }
  if (overview.isError) {
    return (
      <>
        <PageHeader title="Et si ?" description={description} />
        <ErrorState error={overview.error} onRetry={() => void overview.refetch()} />
      </>
    )
  }
  if (overview.data.gross_assets === 0 && overview.data.liabilities === 0) {
    return (
      <>
        <PageHeader title="Et si ?" description={description} />
        <EmptyState title="Aucun patrimoine à simuler">Ajoutez d’abord vos comptes : le scénario s’applique à leurs valeurs.</EmptyState>
      </>
    )
  }

  const result = applyScenario(overview.data, toParams(current))
  const impacts = goalImpacts(goals.data ?? [], accounts.data ?? [], result.ratios)
  const set = (key: Key) => (value: string) => setForm({ ...current, [key]: value })

  return (
    <>
      <PageHeader title="Et si ?" description={description} />
      <div className="grid gap-4 sm:gap-5 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)]">
        <Panel title="Scénario">
          <div role="group" aria-label="Scénarios prédéfinis" className="mb-4 flex flex-wrap gap-2">
            {PRESETS.map((preset) => (
              <Button
                key={preset.id}
                size="sm"
                variant="secondary"
                onClick={() => setForm(toForm({ ...NEUTRAL, ...preset.params }, current.monthlyExpenses))}
              >
                {preset.label}
              </Button>
            ))}
          </div>
          <div className="flex flex-col gap-3">
            {FIELDS.map((field) => (
              <NumberField key={field.key} label={field.label} suffix={field.suffix} value={current[field.key]} onChange={set(field.key)} />
            ))}
          </div>
          <Button className="mt-4" variant="ghost" onClick={() => setForm(null)}>
            Réinitialiser
          </Button>
        </Panel>
        <div className="flex min-w-0 flex-col gap-4 sm:gap-5">
          <Panel title="Effet immédiat">
            <Summary result={result} />
          </Panel>
          <Panel title="Avant / après par catégorie">
            <Comparison result={result} />
          </Panel>
          <Panel title="Objectifs">
            {impacts.length === 0 ? (
              <p className="text-muted">Aucun objectif défini.</p>
            ) : (
              <ul className="flex flex-col gap-4">
                {impacts.map((impact) => (
                  <li key={impact.goal.id}>
                    <div className="mb-1.5 flex flex-wrap items-baseline justify-between gap-2">
                      <span className="font-medium">{impact.goal.name}</span>
                      <span className="num text-[13.5px] text-muted">
                        {pct(impact.pctBefore, 0)} → <span className="text-ink">{pct(impact.pctAfter, 0)}</span>{' '}
                        (<Delta before={impact.before} after={impact.after} />)
                      </span>
                    </div>
                    <ProgressBar pct={impact.pctAfter} tone={impact.pctAfter < impact.pctBefore ? 'brass' : 'accent'} />
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>
      </div>
    </>
  )
}
