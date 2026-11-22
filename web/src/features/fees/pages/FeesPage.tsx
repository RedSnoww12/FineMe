import { useMemo, useState } from 'react'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { useOverview } from '@/shared/api/queries'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import {
  Button,
  Dot,
  IconButton,
  Notice,
  NumberField,
  PageHeader,
  Panel,
  SelectField,
  Stat,
  TextField,
} from '@/shared/ui/ui'
import { parseFrNumber } from '@/shared/lib/parse'
import { RealFeesPanel } from '../components/RealFeesPanel'
import { simulateFees } from '../lib/fees'
import type { AccountCost } from '../lib/portfolioFees'
import { money, moneyCompact, pct, pctFromFraction, toInput } from '@/shared/lib/format'

type ScenarioForm = {
  id: number
  name: string
  grossReturn: string
  entryFee: string
  managementFee: string
  ter: string
  brokerageMode: 'fixed' | 'percent'
  brokerage: string
  performanceFee: string
}

type Preset = Omit<ScenarioForm, 'id' | 'grossReturn'>

// Ordres de grandeur illustratifs, à remplacer par les frais réels de vos contrats.
const PRESETS: Record<string, Preset> = {
  etf_pea: {
    name: 'ETF en PEA',
    entryFee: '0',
    managementFee: '0',
    ter: '0,25',
    brokerageMode: 'percent',
    brokerage: '0,5',
    performanceFee: '0',
  },
  active_av: {
    name: 'Fonds actif en assurance-vie',
    entryFee: '2',
    managementFee: '0,85',
    ter: '1,8',
    brokerageMode: 'fixed',
    brokerage: '0',
    performanceFee: '0',
  },
  etf_av: {
    name: 'ETF en assurance-vie',
    entryFee: '0',
    managementFee: '0,6',
    ter: '0,25',
    brokerageMode: 'fixed',
    brokerage: '0',
    performanceFee: '0',
  },
  managed: {
    name: 'Gestion pilotée',
    entryFee: '0',
    managementFee: '1,6',
    ter: '0,3',
    brokerageMode: 'fixed',
    brokerage: '0',
    performanceFee: '0',
  },
  blank: {
    name: 'Autre',
    entryFee: '0',
    managementFee: '0',
    ter: '0',
    brokerageMode: 'fixed',
    brokerage: '0',
    performanceFee: '0',
  },
}

const PRESET_OPTIONS = [
  { value: '', label: 'Partir d’un exemple…' },
  ...Object.entries(PRESETS).map(([value, preset]) => ({ value, label: preset.name })),
]

const COLORS = ['var(--c1)', 'var(--c2)', 'var(--c3)']
const value = (text: string) => parseFrNumber(text) ?? 0

let nextId = 1
const fromPreset = (key: string, grossReturn: string): ScenarioForm => ({ id: nextId++, grossReturn, ...PRESETS[key] })

export default function FeesPage() {
  const overview = useOverview()
  const [initial, setInitial] = useState('')
  const [monthly, setMonthly] = useState('')
  const [years, setYears] = useState('20')
  const [scenarios, setScenarios] = useState<ScenarioForm[]>(() => [fromPreset('etf_pea', '6'), fromPreset('active_av', '6')])
  const [prefilled, setPrefilled] = useState(false)

  const horizon = Math.min(60, Math.max(1, Math.floor(value(years))))
  const update = (id: number, patch: Partial<ScenarioForm>) =>
    setScenarios((list) => list.map((scenario) => (scenario.id === id ? { ...scenario, ...patch } : scenario)))

  const results = useMemo(
    () =>
      scenarios.map((scenario) =>
        simulateFees({
          initial: Math.max(0, value(initial)),
          monthly: Math.max(0, value(monthly)),
          years: horizon,
          grossReturnPct: value(scenario.grossReturn),
          entryFeePct: Math.max(0, value(scenario.entryFee)),
          managementFeePct: Math.max(0, value(scenario.managementFee)),
          terPct: Math.max(0, value(scenario.ter)),
          brokerage: { mode: scenario.brokerageMode, value: Math.max(0, value(scenario.brokerage)) },
          performanceFeePct: Math.max(0, value(scenario.performanceFee)),
        }),
      ),
    [scenarios, initial, monthly, horizon],
  )

  const sameReturn = scenarios.every((scenario) => value(scenario.grossReturn) === value(scenarios[0].grossReturn))
  const [thisYear] = useState(() => new Date().getFullYear())
  const chartRows = results[0].rows.map((row, i) => {
    const point: Record<string, number> = { year: thisYear + row.year, baseline: row.baseline }
    results.forEach((result, s) => {
      point[`s${scenarios[s].id}`] = result.rows[i].value
    })
    return point
  })
  const hasAmounts = value(initial) > 0 || value(monthly) > 0
  const best = Math.max(...results.map((result) => result.finalValue))
  const ratio = overview.data?.weighted_expense_ratio ?? null

  const prefillFromPortfolio = () => {
    if (!overview.data || ratio === null) return
    setInitial(toInput(Math.max(0, overview.data.financial_assets), 0))
    setScenarios((list) => {
      const mine: ScenarioForm = {
        ...list[0],
        name: 'Mon portefeuille',
        ter: toInput(ratio * 100, 3),
        entryFee: '0',
        performanceFee: '0',
      }
      return [mine, ...list.slice(1)]
    })
    setPrefilled(true)
  }

  // Le premier scénario reprend un compte réel, pour le comparer à une alternative.
  const testAccount = (account: AccountCost) => {
    setInitial(toInput(Math.max(0, account.value), 0))
    setScenarios((list) => {
      const mine: ScenarioForm = {
        ...list[0],
        name: account.name,
        managementFee: toInput(account.envelopePct ?? 0, 3),
        ter: toInput(account.value > 0 ? (account.fundsCost / account.value) * 100 : 0, 3),
        entryFee: '0',
        performanceFee: '0',
      }
      return [mine, ...list.slice(1)]
    })
    setPrefilled(false)
  }

  return (
    <>
      <PageHeader
        title="Simulateur de frais"
        description="Ce que coûtent vos frais réels, et ce que des frais différents coûteraient sur la durée."
      />

      <div className="flex flex-col gap-4 sm:gap-5">
        <RealFeesPanel onTest={testAccount} />
        <Panel title="Versements">
          <div className="grid gap-4 sm:grid-cols-3">
            <NumberField label="Montant initial" value={initial} onChange={setInitial} suffix="€" />
            <NumberField label="Versement mensuel" value={monthly} onChange={setMonthly} suffix="€" />
            <NumberField label="Durée" value={years} onChange={setYears} suffix="ans" hint="De 1 à 60 ans." />
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <Button icon="download" disabled={ratio === null} onClick={prefillFromPortfolio}>
              Préremplir avec mon portefeuille
            </Button>
            <span className="text-[13.5px] text-muted">
              {overview.isPending
                ? 'Chargement de votre portefeuille…'
                : overview.isError
                  ? 'Portefeuille indisponible pour le moment.'
                  : ratio === null
                    ? 'Renseignez les frais courants de vos positions pour utiliser ce raccourci.'
                    : `Frais courants moyens de vos positions : ${pctFromFraction(ratio, 2)}.`}
            </span>
          </div>
          {prefilled ? (
            <div className="mt-3">
              <Notice>
                Le premier scénario reprend vos actifs financiers et vos frais courants moyens. Ajoutez les frais de
                gestion de vos enveloppes : FineMe ne les connaît pas.
              </Notice>
            </div>
          ) : null}
        </Panel>

        <p className="text-[14px] text-muted">
          Les exemples ci-dessous sont des hypothèses illustratives, pas les tarifs d’un établissement. Remplacez-les
          par les frais figurant dans vos contrats.
        </p>

        <div className={`grid gap-4 sm:gap-5 ${scenarios.length === 3 ? 'lg:grid-cols-3' : 'lg:grid-cols-2'}`}>
          {scenarios.map((scenario, index) => {
            const result = results[index]
            return (
              <section key={scenario.id} className="panel flex flex-col gap-4 px-4 py-4 sm:px-5 sm:py-5">
                <header className="flex items-end gap-2">
                  <span className="mb-3.5">
                    <Dot color={COLORS[index]} />
                  </span>
                  <TextField
                    label={`Scénario ${index + 1}`}
                    fieldClassName="flex-1"
                    value={scenario.name}
                    onChange={(name) => update(scenario.id, { name })}
                  />
                  {scenarios.length > 2 ? (
                    <IconButton
                      icon="trash"
                      label={`Retirer le scénario ${scenario.name}`}
                      className="mb-0.5"
                      onClick={() => setScenarios((list) => list.filter((s) => s.id !== scenario.id))}
                    />
                  ) : null}
                </header>

                <SelectField
                  label="Exemple"
                  value=""
                  onChange={(key) => {
                    if (key) update(scenario.id, PRESETS[key])
                  }}
                  options={PRESET_OPTIONS}
                />

                <div className="grid grid-cols-2 gap-x-3 gap-y-4">
                  <NumberField
                    label="Rendement brut"
                    value={scenario.grossReturn}
                    onChange={(grossReturn) => update(scenario.id, { grossReturn })}
                    suffix="%"
                    hint="Par an, avant frais."
                  />
                  <NumberField
                    label="Frais d’entrée"
                    value={scenario.entryFee}
                    onChange={(entryFee) => update(scenario.id, { entryFee })}
                    suffix="%"
                    hint="Sur chaque versement."
                  />
                  <NumberField
                    label="Gestion du contrat"
                    value={scenario.managementFee}
                    onChange={(managementFee) => update(scenario.id, { managementFee })}
                    suffix="%"
                    hint="Par an, sur l’encours."
                  />
                  <NumberField
                    label="Frais courants des fonds"
                    value={scenario.ter}
                    onChange={(ter) => update(scenario.id, { ter })}
                    suffix="%"
                    hint="TER, par an."
                  />
                  <SelectField
                    label="Courtage par ordre"
                    value={scenario.brokerageMode}
                    onChange={(mode) => update(scenario.id, { brokerageMode: mode as ScenarioForm['brokerageMode'] })}
                    options={[
                      { value: 'fixed', label: 'Montant fixe' },
                      { value: 'percent', label: 'Pourcentage' },
                    ]}
                  />
                  <NumberField
                    label="Frais de courtage"
                    value={scenario.brokerage}
                    onChange={(brokerage) => update(scenario.id, { brokerage })}
                    suffix={scenario.brokerageMode === 'fixed' ? '€' : '%'}
                    hint="Un ordre par versement."
                  />
                  <NumberField
                    label="Commission de surperformance"
                    fieldClassName="col-span-2"
                    value={scenario.performanceFee}
                    onChange={(performanceFee) => update(scenario.id, { performanceFee })}
                    suffix="%"
                    hint="Facultatif. Part du gain annuel prélevée, sans seuil."
                  />
                </div>

                <div className="mt-auto grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                  <Stat
                    label="Valeur finale"
                    value={money(result.finalValue)}
                    sub={hasAmounts && results.length > 1 && result.finalValue === best ? 'La plus élevée' : undefined}
                  />
                  <Stat label="Frais payés" value={money(result.totalFees)} />
                  <Stat
                    label="Perdu à cause des frais"
                    value={money(result.lostToFees)}
                    tone={result.lostToFees > 0.5 ? 'neg' : undefined}
                    sub={
                      result.baselineValue > 0
                        ? `${pct((result.lostToFees / result.baselineValue) * 100, 1)} de la valeur sans frais`
                        : undefined
                    }
                  />
                  <Stat label="Sans aucun frais" value={money(result.baselineValue)} sub={`${money(result.contributed)} versés`} />
                </div>
              </section>
            )
          })}
        </div>

        {scenarios.length < 3 ? (
          <div>
            <Button icon="plus" onClick={() => setScenarios((list) => [...list, fromPreset('etf_av', list[0].grossReturn)])}>
              Ajouter un troisième scénario
            </Button>
          </div>
        ) : null}

        <Panel title="Valeur au fil du temps">
          {!hasAmounts ? (
            <p className="text-muted">
              Saisissez un montant initial ou un versement mensuel pour comparer les scénarios.
            </p>
          ) : (
            <>
              <div className="h-[260px] sm:h-[320px]">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={chartRows} margin={{ top: 8, right: 6, bottom: 0, left: 0 }}>
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
                    {sameReturn ? (
                      <Line
                        dataKey="baseline"
                        name="Sans aucun frais"
                        stroke="var(--c7)"
                        strokeDasharray="4 4"
                        strokeWidth={1.5}
                        dot={false}
                        isAnimationActive={false}
                      />
                    ) : null}
                    {scenarios.map((scenario, index) => (
                      <Line
                        key={scenario.id}
                        dataKey={`s${scenario.id}`}
                        name={scenario.name || `Scénario ${index + 1}`}
                        stroke={COLORS[index]}
                        strokeWidth={2.25}
                        dot={false}
                        isAnimationActive={false}
                      />
                    ))}
                  </LineChart>
                </ResponsiveContainer>
              </div>
              <Legend
                items={[
                  ...scenarios.map((scenario, index) => ({
                    label: scenario.name || `Scénario ${index + 1}`,
                    color: COLORS[index],
                  })),
                  ...(sameReturn ? [{ label: 'Sans aucun frais', color: 'var(--c7)' }] : []),
                ]}
              />
            </>
          )}
          <p className="mt-4 max-w-[72ch] text-[13.5px] text-muted">
            « Perdu à cause des frais » compare chaque scénario au même placement sans aucun frais : il inclut les frais
            payés et le rendement qu’ils auraient produit. Fiscalité et inflation ne sont pas prises en compte.
          </p>
        </Panel>
      </div>
    </>
  )
}
