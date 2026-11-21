import { useMemo, useState } from 'react'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { Loan, ScheduleRow } from '@/shared/api/types'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import { Button, ErrorState, LoadingBlock, Modal, NumberField, Segmented, SelectField, Stat } from '@/shared/ui/ui'
import { date, duration, money, moneyCompact, todayIso } from '@/shared/lib/format'
import { useLoanSchedule } from '../hooks/useRealEstateApi'
import { readField } from '../lib/fields'
import {
  compareSchedules,
  prepaymentPenaltyCap,
  remainingAfter,
  simulateSchedule,
  type LoanEvent,
  type LoanTerms,
  type PrepaymentMode,
  type SimulatedSchedule,
} from '../lib/schedule'

type Row = ScheduleRow & { prepayment?: number }
type YearRow = { year: string; payment: number; interest: number; principal: number; insurance: number; prepayment: number; remaining: number }

function byYear(rows: Row[]): YearRow[] {
  const years = new Map<string, YearRow>()
  for (const row of rows) {
    const year = row.date.slice(0, 4)
    const current = years.get(year) ?? { year, payment: 0, interest: 0, principal: 0, insurance: 0, prepayment: 0, remaining: 0 }
    current.payment += row.payment
    current.interest += row.interest
    current.principal += row.principal
    current.insurance += row.insurance
    current.prepayment += row.prepayment ?? 0
    current.remaining = row.remaining
    years.set(year, current)
  }
  return [...years.values()]
}

function termsOf(loan: Loan): LoanTerms {
  return {
    principal: loan.principal,
    interestRatePct: loan.interest_rate_pct,
    insuranceRatePct: loan.insurance_rate_pct,
    durationMonths: loan.duration_months,
    startDate: loan.start_date,
  }
}

type Scenario = 'none' | 'prepayment' | 'renegotiation'
type ScenarioInput = { scenario: Scenario; afterMonth: string; full: 'partial' | 'total'; amount: string; mode: PrepaymentMode; rate: string; fees: string }

const EMPTY_SCENARIO: ScenarioInput = { scenario: 'none', afterMonth: '', full: 'partial', amount: '', mode: 'reduce_duration', rate: '', fees: '' }

/** Événement décrit par la saisie, ou `null` tant qu'elle est incomplète. */
function eventOf(input: ScenarioInput, durationMonths: number): LoanEvent | null {
  const afterMonth = readField(input.afterMonth, { max: durationMonths - 1 })
  if (afterMonth === null || !Number.isInteger(afterMonth)) return null
  if (input.scenario === 'prepayment') {
    if (input.full === 'total') return { kind: 'full_repayment', afterMonth }
    const amount = readField(input.amount)
    return amount !== null && amount > 0 ? { kind: 'prepayment', afterMonth, amount, mode: input.mode } : null
  }
  if (input.scenario === 'renegotiation') {
    const rate = readField(input.rate)
    return rate !== null ? { kind: 'renegotiation', afterMonth, interestRatePct: rate } : null
  }
  return null
}

function paidInstallments(rows: ScheduleRow[], today: string): number {
  return rows.filter((row) => row.date <= today).length
}

function ScenarioForm({
  input,
  onChange,
  loan,
  paid,
}: {
  input: ScenarioInput
  onChange: (input: ScenarioInput) => void
  loan: Loan
  paid: number
}) {
  const set = (patch: Partial<ScenarioInput>) => onChange({ ...input, ...patch })
  return (
    <div className="flex flex-col gap-4 rounded-[11px] border border-line p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="text-[15px] font-semibold">Simuler une modification</h3>
        <Segmented
          label="Modification simulée"
          value={input.scenario}
          onChange={(scenario) => set({ scenario })}
          options={[
            { value: 'none', label: 'Aucune' },
            { value: 'prepayment', label: 'Remboursement anticipé' },
            { value: 'renegotiation', label: 'Renégociation' },
          ]}
        />
      </div>
      {input.scenario === 'none' ? (
        <p className="text-[14px] text-muted">
          Choisissez un remboursement anticipé ou une renégociation : l’échéancier ci-dessous est recalculé.
        </p>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <NumberField
            label="Après l’échéance n°"
            hint={`${paid} échéance${paid > 1 ? 's' : ''} déjà payée${paid > 1 ? 's' : ''} sur ${loan.duration_months}. 0 = avant la première.`}
            value={input.afterMonth}
            onChange={(afterMonth) => set({ afterMonth })}
          />
          {input.scenario === 'prepayment' ? (
            <>
              <SelectField
                label="Remboursement"
                value={input.full}
                onChange={(full) => set({ full: full === 'total' ? 'total' : 'partial' })}
                options={[
                  { value: 'partial', label: 'Partiel' },
                  { value: 'total', label: 'Total' },
                ]}
              />
              {input.full === 'partial' ? (
                <>
                  <NumberField label="Montant remboursé" suffix="€" value={input.amount} onChange={(amount) => set({ amount })} />
                  <SelectField
                    label="Ensuite"
                    value={input.mode}
                    onChange={(mode) => set({ mode: mode === 'reduce_payment' ? 'reduce_payment' : 'reduce_duration' })}
                    options={[
                      { value: 'reduce_duration', label: 'Réduire la durée' },
                      { value: 'reduce_payment', label: 'Réduire la mensualité' },
                    ]}
                  />
                </>
              ) : null}
            </>
          ) : (
            <>
              <NumberField label="Nouveau taux nominal" suffix="%" value={input.rate} onChange={(rate) => set({ rate })} />
              <NumberField label="Frais de renégociation" suffix="€" hint="Dossier, garantie. Vide si aucun." value={input.fees} onChange={(fees) => set({ fees })} />
            </>
          )}
        </div>
      )}
    </div>
  )
}

function ScenarioResults({
  input,
  event,
  base,
  simulated,
  loan,
}: {
  input: ScenarioInput
  event: LoanEvent
  base: SimulatedSchedule
  simulated: SimulatedSchedule
  loan: Loan
}) {
  const diff = compareSchedules(base, simulated)
  const remainingBefore = remainingAfter(base, loan.principal, event.afterMonth)
  const fees = readField(input.fees, { optional: true }) ?? 0
  const penalty = event.kind === 'renegotiation' ? 0 : prepaymentPenaltyCap(simulated.totalPrepaid, remainingBefore, loan.interest_rate_pct)
  const saved = diff.interestSaved + diff.insuranceSaved
  const net = saved - (event.kind === 'renegotiation' ? fees : penalty)
  // Soldé par anticipation : la dernière ligne porte le remboursement, ou il a eu lieu avant toute échéance.
  const paidOff = simulated.months === 0 || (simulated.rows.at(-1)?.prepayment ?? 0) > 0
  return (
    <div className="grid grid-cols-2 gap-x-6 gap-y-4 rounded-[11px] bg-surface-2 p-4 sm:grid-cols-4">
      <Stat
        label="Intérêts et assurance économisés"
        value={money(saved)}
        tone={saved > 0 ? 'pos' : undefined}
        sub={`${money(diff.interestSaved)} d’intérêts`}
      />
      <Stat
        label="Nouvelle fin"
        value={date(simulated.endDate, 'month')}
        sub={diff.monthsSaved > 0 ? `${duration(diff.monthsSaved)} plus tôt` : 'durée inchangée'}
      />
      <Stat
        label="Nouvelle mensualité"
        value={paidOff ? 'Prêt soldé' : money(simulated.finalMonthlyPayment, { decimals: 2 })}
        sub={
          paidOff
            ? `après l’échéance ${event.afterMonth}`
            : Math.abs(diff.monthlyPaymentChange) >= 0.005
            ? `${money(diff.monthlyPaymentChange, { decimals: 2, signed: true })} par mois, hors assurance`
            : 'hors assurance'
        }
      />
      {event.kind === 'renegotiation' ? (
        <Stat label="Gain net des frais" value={money(net)} tone={net > 0 ? 'pos' : net < 0 ? 'neg' : undefined} />
      ) : (
        <Stat
          label="Indemnités, plafond légal"
          value={money(penalty)}
          sub={`${money(simulated.totalPrepaid)} remboursés par anticipation`}
        />
      )}
    </div>
  )
}

export function ScheduleModal({ accountId, name, onClose }: { accountId: string; name: string; onClose: () => void }) {
  const schedule = useLoanSchedule(accountId)
  const [grain, setGrain] = useState<'year' | 'month'>('year')
  const [scenario, setScenario] = useState<ScenarioInput>(EMPTY_SCENARIO)
  const loan = schedule.data?.loan
  const apiRows = useMemo(() => schedule.data?.rows ?? [], [schedule.data])

  const simulation = useMemo(() => {
    const event = loan ? eventOf(scenario, loan.duration_months) : null
    if (!loan || !event) return null
    const terms = termsOf(loan)
    return { event, base: simulateSchedule(terms), simulated: simulateSchedule(terms, [event]) }
  }, [loan, scenario])
  const rows: Row[] = simulation ? simulation.simulated.rows : apiRows
  const yearly = useMemo(() => byYear(rows), [rows])
  const hasPrepayment = rows.some((row) => (row.prepayment ?? 0) > 0)
  const chartData = useMemo(() => {
    if (!simulation) return apiRows.map((row) => ({ date: row.date, remaining: row.remaining }))
    const simulatedByMonth = new Map(simulation.simulated.rows.map((row) => [row.month, row.remaining]))
    return simulation.base.rows.map((row) => ({
      date: row.date,
      remaining: row.remaining,
      simulated: simulatedByMonth.get(row.month) ?? 0,
    }))
  }, [apiRows, simulation])

  return (
    <Modal title={`Amortissement : ${name}`} wide onClose={onClose} footer={<Button onClick={onClose}>Fermer</Button>}>
      {schedule.isPending ? (
        <LoadingBlock />
      ) : schedule.isError ? (
        <ErrorState error={schedule.error} onRetry={() => void schedule.refetch()} />
      ) : !loan || apiRows.length === 0 ? (
        <p className="text-muted">Le tableau d’amortissement de ce prêt est vide.</p>
      ) : (
        <div className="flex flex-col gap-5">
          <div className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4">
            <Stat label="Restant dû" value={money(loan.remaining_capital)} />
            <Stat
              label="Mensualité"
              value={money(loan.monthly_payment + loan.monthly_insurance, { decimals: 2 })}
              sub={loan.monthly_insurance > 0 ? `dont ${money(loan.monthly_insurance, { decimals: 2 })} d’assurance` : undefined}
            />
            <Stat label="Intérêts totaux" value={money(loan.total_interest)} />
            <Stat label="Assurance totale" value={money(loan.total_insurance)} sub={`fin ${date(loan.end_date, 'month')}`} />
          </div>

          <ScenarioForm input={scenario} onChange={setScenario} loan={loan} paid={paidInstallments(apiRows, todayIso())} />
          {simulation ? (
            <ScenarioResults input={scenario} event={simulation.event} base={simulation.base} simulated={simulation.simulated} loan={loan} />
          ) : null}

          <div>
            <div className="h-[220px]">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 8, right: 4, bottom: 0, left: 0 }}>
                  <CartesianGrid stroke="var(--line)" vertical={false} />
                  <XAxis
                    dataKey="date"
                    tickFormatter={(value: string) => value.slice(0, 4)}
                    tick={AXIS_TICK}
                    tickLine={false}
                    axisLine={{ stroke: 'var(--line)' }}
                    minTickGap={44}
                  />
                  <YAxis tickFormatter={(value: number) => moneyCompact(value)} tick={AXIS_TICK} tickLine={false} axisLine={false} width={62} />
                  <Tooltip content={(props) => <MoneyTooltip {...props} formatLabel={(label) => date(String(label), 'month')} />} />
                  <Line type="monotone" dataKey="remaining" name="Capital restant dû" stroke="var(--c1)" strokeWidth={2} dot={false} isAnimationActive={false} />
                  {simulation ? (
                    <Line type="monotone" dataKey="simulated" name="Après modification" stroke="var(--c2)" strokeWidth={2} dot={false} isAnimationActive={false} />
                  ) : null}
                </LineChart>
              </ResponsiveContainer>
            </div>
            <Legend
              items={[
                { label: 'Capital restant dû', color: 'var(--c1)' },
                ...(simulation ? [{ label: 'Après modification', color: 'var(--c2)' }] : []),
              ]}
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <h3 className="text-[15px] font-semibold">{simulation ? 'Échéancier simulé' : 'Échéancier'}</h3>
            <Segmented
              label="Détail de l’échéancier"
              value={grain}
              onChange={setGrain}
              options={[
                { value: 'year', label: 'Par année' },
                { value: 'month', label: 'Par mois' },
              ]}
            />
          </div>
          <div className="table-wrap max-h-[360px] overflow-y-auto rounded-[11px] border border-line">
            <table className="table">
              <thead className="sticky top-0 bg-surface">
                <tr>
                  <th>{grain === 'year' ? 'Année' : 'Échéance'}</th>
                  <th className="r">Payé</th>
                  <th className="r">Capital</th>
                  <th className="r">Intérêts</th>
                  <th className="r">Assurance</th>
                  {hasPrepayment ? <th className="r">Anticipé</th> : null}
                  <th className="r">Restant dû</th>
                </tr>
              </thead>
              <tbody>
                {grain === 'year'
                  ? yearly.map((row) => (
                      <tr key={row.year}>
                        <td className="num">{row.year}</td>
                        <td className="r">{money(row.payment)}</td>
                        <td className="r">{money(row.principal)}</td>
                        <td className="r">{money(row.interest)}</td>
                        <td className="r">{money(row.insurance)}</td>
                        {hasPrepayment ? <td className="r">{money(row.prepayment)}</td> : null}
                        <td className="r font-medium">{money(row.remaining)}</td>
                      </tr>
                    ))
                  : rows.map((row) => (
                      <tr key={row.month}>
                        <td className="num whitespace-nowrap">
                          {row.month}. {date(row.date, 'month')}
                        </td>
                        <td className="r">{money(row.payment, { decimals: 2 })}</td>
                        <td className="r">{money(row.principal, { decimals: 2 })}</td>
                        <td className="r">{money(row.interest, { decimals: 2 })}</td>
                        <td className="r">{money(row.insurance, { decimals: 2 })}</td>
                        {hasPrepayment ? <td className="r">{money(row.prepayment ?? 0, { decimals: 2 })}</td> : null}
                        <td className="r font-medium">{money(row.remaining, { decimals: 2 })}</td>
                      </tr>
                    ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Modal>
  )
}
