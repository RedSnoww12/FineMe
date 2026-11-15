import { useMemo, useState } from 'react'
import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { useGoals, useOverview } from '@/shared/api/queries'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import { Button, FormError, NumberField, PageHeader, Panel, Segmented, SelectField, Stat } from '@/shared/ui/ui'
import { parseFrNumber } from '@/shared/lib/parse'
import { fireTarget, monthsToReach, projectGrowth } from '../lib/projection'
import { duration, money, moneyCompact, pct, toInput } from '@/shared/lib/format'

type Mode = 'nominal' | 'real'

const SCENARIOS = [
  { key: 'pessimistic', label: 'Pessimiste', color: 'var(--c6)' },
  { key: 'central', label: 'Central', color: 'var(--c1)' },
  { key: 'optimistic', label: 'Optimiste', color: 'var(--c3)' },
] as const

const value = (text: string) => parseFrNumber(text) ?? 0

export default function ProjectionPage() {
  const overview = useOverview()
  const goals = useGoals()

  // null = pas encore saisi : on reprend alors les actifs financiers de la vue courante.
  const [capitalInput, setCapitalInput] = useState<string | null>(null)
  const [monthly, setMonthly] = useState('')
  const [returnPct, setReturnPct] = useState('5')
  const [spread, setSpread] = useState('2')
  const [inflation, setInflation] = useState('2')
  const [years, setYears] = useState('20')
  const [mode, setMode] = useState<Mode>('real')
  const [expenses, setExpenses] = useState('')
  const [withdrawal, setWithdrawal] = useState('4')
  const [goalId, setGoalId] = useState('')

  const capitalText = capitalInput ?? (overview.data ? toInput(Math.max(0, overview.data.financial_assets), 0) : '')
  const horizon = Math.min(60, Math.max(1, Math.floor(value(years))))
  const central = value(returnPct)
  const gap = Math.abs(value(spread))
  const inflationPct = value(inflation)
  const initial = Math.max(0, value(capitalText))
  const monthlyAmount = value(monthly)
  const [thisYear] = useState(() => new Date().getFullYear())

  const rates = useMemo(
    () => ({ pessimistic: central - gap, central, optimistic: central + gap }),
    [central, gap],
  )

  const rows = useMemo(() => {
    const series = {
      pessimistic: projectGrowth({ initial, monthly: monthlyAmount, annualReturnPct: rates.pessimistic, inflationPct, years: horizon }),
      central: projectGrowth({ initial, monthly: monthlyAmount, annualReturnPct: rates.central, inflationPct, years: horizon }),
      optimistic: projectGrowth({ initial, monthly: monthlyAmount, annualReturnPct: rates.optimistic, inflationPct, years: horizon }),
    }
    return series.central.map((row, i) => ({
      year: thisYear + row.year,
      contributed: row.contributed,
      pessimistic: series.pessimistic[i][mode],
      central: row[mode],
      optimistic: series.optimistic[i][mode],
    }))
  }, [initial, monthlyAmount, rates, inflationPct, horizon, mode, thisYear])

  const last = rows[rows.length - 1]
  const annualExpenses = value(expenses)
  const target = annualExpenses > 0 ? fireTarget(annualExpenses, value(withdrawal)) : null
  const reach = SCENARIOS.map((scenario) => ({
    ...scenario,
    months:
      target !== null
        ? monthsToReach({ initial, monthly: monthlyAmount, annualReturnPct: rates[scenario.key], inflationPct, target })
        : null,
  }))

  const invalid = [capitalText, monthly, returnPct, spread, inflation, years].some(
    (text) => text.trim() !== '' && parseFrNumber(text) === null,
  )
  const projects = (goals.data ?? []).filter((goal) => goal.kind === 'project')

  const applyGoal = (id: string) => {
    setGoalId(id)
    const goal = projects.find((g) => g.id === id)
    if (!goal) return
    if (goal.monthly_contribution !== null) setMonthly(toInput(goal.monthly_contribution, 2))
    if (goal.expected_return_pct !== null) setReturnPct(toInput(goal.expected_return_pct))
    setCapitalInput(toInput(goal.progress.current_amount, 0))
    if (goal.target_date) {
      const span = Number(goal.target_date.slice(0, 4)) - thisYear
      if (span >= 1) setYears(String(Math.min(60, span)))
    }
  }

  return (
    <>
      <PageHeader
        title="Projection"
        description="Une estimation à partir de vos hypothèses, calculée dans votre navigateur. Les rendements passés ou supposés ne garantissent rien."
      />

      <div className="flex flex-col gap-4 sm:gap-5">
        <Panel title="Hypothèses">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <div className="flex flex-col gap-2">
              <NumberField label="Capital de départ" value={capitalText} onChange={setCapitalInput} suffix="€" />
              {overview.data ? (
                <div className="flex flex-wrap gap-2">
                  <Button size="sm" onClick={() => setCapitalInput(toInput(Math.max(0, overview.data.financial_assets), 0))}>
                    Actifs financiers
                  </Button>
                  <Button size="sm" onClick={() => setCapitalInput(toInput(Math.max(0, overview.data.net_worth), 0))}>
                    Patrimoine net
                  </Button>
                </div>
              ) : overview.isError ? (
                <p className="text-[13px] text-muted">Patrimoine indisponible : saisissez le capital à la main.</p>
              ) : null}
            </div>
            <NumberField label="Épargne mensuelle" value={monthly} onChange={setMonthly} suffix="€" hint="Versée chaque fin de mois." />
            <NumberField label="Horizon" value={years} onChange={setYears} suffix="ans" hint="De 1 à 60 ans." />
            <NumberField
              label="Rendement annuel attendu"
              value={returnPct}
              onChange={setReturnPct}
              suffix="%"
              hint="Scénario central, net de frais."
            />
            <NumberField
              label="Écart des scénarios"
              value={spread}
              onChange={setSpread}
              suffix="pts"
              hint={`Pessimiste ${pct(rates.pessimistic, 1)}, optimiste ${pct(rates.optimistic, 1)}.`}
            />
            <NumberField label="Inflation annuelle" value={inflation} onChange={setInflation} suffix="%" />
            {projects.length > 0 ? (
              <SelectField
                label="Reprendre un objectif"
                value={goalId}
                onChange={applyGoal}
                options={[{ value: '', label: 'Aucun' }, ...projects.map((goal) => ({ value: goal.id, label: goal.name }))]}
                hint="Remplit le capital, le versement, le rendement et l’horizon."
              />
            ) : null}
          </div>
          {invalid ? (
            <div className="mt-4">
              <FormError error="Une valeur n’est pas un nombre : elle est comptée comme 0." />
            </div>
          ) : null}
        </Panel>

        <Panel
          title={`Capital en ${last.year}`}
          action={
            <Segmented
              label="Unité des montants"
              value={mode}
              onChange={setMode}
              options={[
                { value: 'real', label: 'Euros d’aujourd’hui' },
                { value: 'nominal', label: 'Euros courants' },
              ]}
            />
          }
        >
          <div className="grid grid-cols-2 gap-x-6 gap-y-5 md:grid-cols-4">
            {SCENARIOS.map((scenario) => (
              <Stat
                key={scenario.key}
                label={`${scenario.label} (${pct(rates[scenario.key], 1)})`}
                value={money(last[scenario.key])}
              />
            ))}
            <Stat label="Total versé" value={money(last.contributed)} sub="Capital de départ compris" />
          </div>
          <p className="mt-3 text-[13.5px] text-muted">
            {mode === 'real'
              ? `Montants corrigés d’une inflation de ${pct(inflationPct, 1)} par an : ce que ce capital permettrait d’acheter aujourd’hui.`
              : 'Montants tels qu’ils s’afficheront sur vos relevés, sans correction de l’inflation.'}
          </p>

          <div className="mt-5 h-[260px] sm:h-[320px]">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={rows} margin={{ top: 8, right: 6, bottom: 0, left: 0 }}>
                <CartesianGrid stroke="var(--line)" vertical={false} />
                <XAxis dataKey="year" interval="preserveStartEnd" tick={AXIS_TICK} tickLine={false} axisLine={{ stroke: 'var(--line)' }} minTickGap={28} />
                <YAxis
                  tickFormatter={(v: number) => moneyCompact(v)}
                  tick={AXIS_TICK}
                  tickLine={false}
                  axisLine={false}
                  width={62}
                />
                <Tooltip content={(props) => <MoneyTooltip {...props} />} />
                {target !== null ? (
                  <ReferenceLine
                    y={target}
                    stroke="var(--brass)"
                    strokeDasharray="5 4"
                    label={{ value: 'Indépendance', position: 'insideTopLeft', fill: 'var(--brass)', fontSize: 12 }}
                  />
                ) : null}
                <Line
                  dataKey="contributed"
                  name="Total versé"
                  stroke="var(--c7)"
                  strokeDasharray="4 4"
                  strokeWidth={1.5}
                  dot={false}
                  isAnimationActive={false}
                />
                {SCENARIOS.map((scenario) => (
                  <Line
                    key={scenario.key}
                    dataKey={scenario.key}
                    name={scenario.label}
                    stroke={scenario.color}
                    strokeWidth={scenario.key === 'central' ? 2.5 : 1.75}
                    dot={false}
                    isAnimationActive={false}
                  />
                ))}
              </LineChart>
            </ResponsiveContainer>
          </div>
          <Legend
            items={[
              ...SCENARIOS.map((scenario) => ({ label: scenario.label, color: scenario.color })),
              { label: 'Total versé', color: 'var(--c7)' },
              ...(target !== null ? [{ label: 'Capital d’indépendance', color: 'var(--brass)' }] : []),
            ]}
          />
        </Panel>

        <Panel title="Indépendance financière">
          <p className="mb-4 max-w-[68ch] text-muted">
            Le capital à partir duquel vos placements couvrent vos dépenses, selon le taux de retrait annuel que vous
            jugez soutenable. Les durées sont calculées en euros d’aujourd’hui.
          </p>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <NumberField label="Dépenses annuelles" value={expenses} onChange={setExpenses} suffix="€" />
            <NumberField
              label="Taux de retrait annuel"
              value={withdrawal}
              onChange={setWithdrawal}
              suffix="%"
              hint="4 % est un repère courant, pas une garantie."
            />
          </div>
          {target === null ? (
            <p className="mt-5 border-t border-line pt-4 text-muted">
              {annualExpenses > 0
                ? 'Saisissez un taux de retrait supérieur à 0.'
                : 'Saisissez vos dépenses annuelles pour connaître le capital à viser et l’année où vous l’atteindrez.'}
            </p>
          ) : (
            <div className="mt-5 grid grid-cols-2 gap-x-6 gap-y-5 border-t border-line pt-5 md:grid-cols-4">
              <Stat label="Capital à viser" value={money(target)} sub={`${money(annualExpenses / 12)} par mois`} />
              {reach.map((scenario) => (
                <Stat
                  key={scenario.key}
                  label={scenario.label}
                  value={
                    scenario.months === null
                      ? 'Hors de portée'
                      : scenario.months === 0
                        ? 'Déjà atteint'
                        : String(thisYear + Math.ceil(scenario.months / 12))
                  }
                  sub={
                    scenario.months === null
                      ? 'Pas avant 80 ans'
                      : scenario.months > 0
                        ? `dans ${duration(scenario.months)}`
                        : undefined
                  }
                />
              ))}
            </div>
          )}
        </Panel>

        <Panel title="Année par année" flush>
          <div className="table-wrap max-h-[480px] overflow-y-auto">
            <table className="table">
              <thead className="sticky top-0 bg-surface">
                <tr>
                  <th>Année</th>
                  <th className="r">Total versé</th>
                  <th className="r">Pessimiste</th>
                  <th className="r">Central</th>
                  <th className="r">Optimiste</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.year}>
                    <td className="num">{row.year}</td>
                    <td className="r text-muted">{money(row.contributed)}</td>
                    <td className="r">{money(row.pessimistic)}</td>
                    <td className="r font-medium">{money(row.central)}</td>
                    <td className="r">{money(row.optimistic)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      </div>
    </>
  )
}
