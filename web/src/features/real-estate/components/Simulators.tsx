import { useState, type ReactNode } from 'react'
import type { RealEstateItem } from '@/shared/api/types'
import { NumberField, Panel, Segmented, SelectField, Signed, Stat } from '@/shared/ui/ui'
import { duration, money, pct, toInput } from '@/shared/lib/format'
import { useRealEstate } from '../hooks/useRealEstateApi'
import { HCSF_MAX_DEBT_RATIO_PCT, borrowingCapacity } from '../lib/borrowing'
import { compareBuyVsRent } from '../lib/buyVsRent'
import { readAll, readField } from '../lib/fields'
import { loanSummary } from '../lib/loan'
import { simulateRental } from '../lib/rental'

const MAX_YEARS = 50

type Form<K extends string> = Record<K, string>

function useForm<K extends string>(keys: readonly K[]) {
  const [values, setValues] = useState<Form<K>>(() => Object.fromEntries(keys.map((key) => [key, ''])) as Form<K>)
  const bind = (key: K) => ({ value: values[key], onChange: (value: string) => setValues((v) => ({ ...v, [key]: value })) })
  const fill = (next: Partial<Form<K>>) => setValues((v) => ({ ...v, ...next }))
  return { values, bind, fill }
}

function Results({ ready, empty, children }: { ready: boolean; empty: string; children: ReactNode }) {
  return ready ? (
    <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-5 border-t border-line pt-5 md:grid-cols-4">{children}</div>
  ) : (
    <p className="mt-5 border-t border-line pt-4 text-muted">{empty}</p>
  )
}

function FieldGroup({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="min-w-0">
      <legend className="mb-2 text-[14px] font-semibold">{legend}</legend>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{children}</div>
    </fieldset>
  )
}

/** Valeurs de départ tirées d'un bien enregistré et de son premier prêt. */
function prefillFromProperty(item: RealEstateItem) {
  const { property } = item
  const loan = item.loans[0]
  const cost = property.purchase_price + property.purchase_fees
  return {
    price: toInput(property.purchase_price),
    fees: toInput(property.purchase_fees),
    rent: toInput(property.monthly_rent),
    charges: toInput(property.monthly_charges),
    tax: toInput(property.yearly_property_tax),
    downPayment: toInput(Math.max(0, cost - (loan?.principal ?? 0))),
    rate: loan ? toInput(loan.interest_rate_pct) : '',
    insurance: loan ? toInput(loan.insurance_rate_pct) : '',
    years: loan ? toInput(loan.duration_months / 12, 2) : '',
  }
}

function PropertyPicker({ onPick }: { onPick: (item: RealEstateItem) => void }) {
  const realEstate = useRealEstate()
  const properties = realEstate.data?.properties ?? []
  const [selected, setSelected] = useState('')
  if (properties.length === 0) return null
  return (
    <SelectField
      label="Partir d’un bien enregistré"
      fieldClassName="mb-5 max-w-sm"
      value={selected}
      onChange={(id) => {
        setSelected(id)
        const item = properties.find((p) => p.account.id === id)
        if (item) onPick(item)
      }}
      options={[{ value: '', label: 'Saisie libre' }, ...properties.map((p) => ({ value: p.account.id, label: p.account.name }))]}
      hint="Reprend le prix, les frais, le loyer, les charges et le premier prêt lié."
    />
  )
}

const RENTAL_KEYS = [
  'price', 'fees', 'works', 'downPayment', 'rate', 'insurance', 'years',
  'rent', 'vacancy', 'charges', 'tax', 'horizon', 'appreciation', 'indexation', 'saleFees',
] as const

function RentalSimulator() {
  const { values, bind, fill } = useForm(RENTAL_KEYS)
  const read = readAll({
    price: readField(values.price),
    fees: readField(values.fees, { optional: true }),
    works: readField(values.works, { optional: true }),
    downPayment: readField(values.downPayment),
    rate: readField(values.rate, { optional: true }),
    insurance: readField(values.insurance, { optional: true }),
    years: readField(values.years, { optional: true, max: MAX_YEARS }),
    rent: readField(values.rent),
    vacancy: readField(values.vacancy, { optional: true, max: 100 }),
    charges: readField(values.charges, { optional: true }),
    tax: readField(values.tax, { optional: true }),
    horizon: readField(values.horizon, { max: MAX_YEARS }),
    appreciation: readField(values.appreciation, { optional: true, signed: true }),
    indexation: readField(values.indexation, { optional: true, signed: true }),
    saleFees: readField(values.saleFees, { optional: true, max: 100 }),
  })
  const needsLoan = read !== null && read.price + read.fees + read.works > read.downPayment
  const ready = read !== null && read.price > 0 && read.horizon >= 1 && (!needsLoan || read.years > 0)
  const result =
    ready && read
      ? simulateRental({
          price: read.price,
          acquisitionFees: read.fees,
          works: read.works,
          downPayment: read.downPayment,
          loanRatePct: read.rate,
          insuranceRatePct: read.insurance,
          loanMonths: Math.round(read.years * 12),
          monthlyRent: read.rent,
          vacancyPct: read.vacancy,
          monthlyCharges: read.charges,
          yearlyPropertyTax: read.tax,
          horizonYears: Math.round(read.horizon),
          appreciationPct: read.appreciation,
          rentIndexationPct: read.indexation,
          saleFeesPct: read.saleFees,
        })
      : null

  return (
    <Panel title="Investissement locatif">
      <p className="mb-4 max-w-[68ch] text-muted">
        Rendements, cash-flow et taux de rendement interne d’un bien loué puis revendu, avant impôts. Les champs
        facultatifs laissés vides comptent pour zéro.
      </p>
      <PropertyPicker onPick={(item) => fill(prefillFromProperty(item))} />
      <div className="flex flex-col gap-5">
        <FieldGroup legend="Acquisition">
          <NumberField label="Prix d’achat" suffix="€" {...bind('price')} />
          <NumberField label="Frais d’acquisition" suffix="€" hint="Notaire, agence, garantie." {...bind('fees')} />
          <NumberField label="Travaux" suffix="€" {...bind('works')} />
          <NumberField label="Apport personnel" suffix="€" hint="Le reste est emprunté." {...bind('downPayment')} />
        </FieldGroup>
        <FieldGroup legend="Prêt">
          <NumberField label="Taux nominal annuel" suffix="%" {...bind('rate')} />
          <NumberField label="Taux d’assurance annuel" suffix="%" hint="Sur le capital initial." {...bind('insurance')} />
          <NumberField label="Durée du prêt" suffix="ans" {...bind('years')} />
        </FieldGroup>
        <FieldGroup legend="Exploitation">
          <NumberField label="Loyer mensuel hors charges" suffix="€" {...bind('rent')} />
          <NumberField label="Vacance locative" suffix="%" hint="Part de l’année sans locataire." {...bind('vacancy')} />
          <NumberField label="Charges mensuelles non récupérables" suffix="€" hint="Copropriété, gestion, assurance." {...bind('charges')} />
          <NumberField label="Taxe foncière annuelle" suffix="€" {...bind('tax')} />
        </FieldGroup>
        <FieldGroup legend="Horizon et revente">
          <NumberField label="Horizon de détention" suffix="ans" {...bind('horizon')} />
          <NumberField label="Revalorisation annuelle du bien" suffix="%" {...bind('appreciation')} />
          <NumberField label="Hausse annuelle des loyers et charges" suffix="%" {...bind('indexation')} />
          <NumberField label="Frais de revente" suffix="%" hint="En % du prix de vente." {...bind('saleFees')} />
        </FieldGroup>
      </div>
      <Results
        ready={result !== null}
        empty="Renseignez le prix, l’apport, le loyer, l’horizon et, si vous empruntez, la durée du prêt (50 ans au plus)."
      >
        {result ? (
          <>
            <Stat label="Rendement brut" value={pct(result.grossYieldPct, 2)} sub={`sur ${money(result.totalCost)} de prix de revient`} />
            <Stat label="Rendement net de charges" value={pct(result.netYieldPct, 2)} />
            <Stat
              label="Cash-flow mensuel"
              value={<Signed value={result.monthlyCashFlow}>{money(result.monthlyCashFlow, { signed: true })}</Signed>}
              sub={result.loanAmount > 0 ? `après ${money(result.monthlyLoanPayment, { decimals: 2 })} de mensualité` : 'sans emprunt'}
            />
            <Stat
              label="Effort d’épargne mensuel"
              value={money(result.monthlySavingsEffort)}
              sub={result.monthlySavingsEffort > 0 ? 'à compléter chaque mois' : 'le loyer couvre tout'}
            />
            <Stat label="Montant emprunté" value={money(result.loanAmount)} sub={`apport ${money(result.invested)}`} />
            <Stat
              label="Revente nette"
              value={money(result.netSaleProceeds)}
              sub={`${money(result.salePrice)} moins ${money(result.remainingDebtAtSale)} restant dû et frais`}
            />
            <Stat
              label="Gain net sur l’horizon"
              value={<Signed value={result.netGain}>{money(result.netGain, { signed: true })}</Signed>}
              sub="cash-flows et revente, apport déduit"
            />
            <Stat
              label={`TRI sur ${duration(result.years.length * 12)}`}
              value={result.irrPct === null ? 'Non défini' : pct(result.irrPct, 2)}
              sub={result.irrPct === null ? 'aucun apport engagé' : 'taux de rendement interne annuel'}
            />
          </>
        ) : null}
      </Results>
    </Panel>
  )
}

const BORROWING_KEYS = ['income', 'debts', 'ratio', 'rate', 'insurance', 'years'] as const

function BorrowingSimulator() {
  const { values, bind } = useForm(BORROWING_KEYS)
  const read = readAll({
    income: readField(values.income),
    debts: readField(values.debts, { optional: true }),
    ratio: readField(values.ratio, { max: 100 }),
    rate: readField(values.rate),
    insurance: readField(values.insurance, { optional: true }),
    years: readField(values.years, { max: MAX_YEARS }),
  })
  const result =
    read && read.income > 0 && read.years > 0
      ? borrowingCapacity({
          monthlyIncome: read.income,
          existingMonthlyDebts: read.debts,
          maxDebtRatioPct: read.ratio,
          annualRatePct: read.rate,
          insuranceRatePct: read.insurance,
          months: Math.round(read.years * 12),
        })
      : null

  return (
    <Panel title="Capacité d’emprunt">
      <p className="mb-4 max-w-[68ch] text-muted">
        Le capital que vous pouvez emprunter sans dépasser le taux d’endettement retenu, assurance comprise.
      </p>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <NumberField label="Revenus nets mensuels du foyer" suffix="€" {...bind('income')} />
        <NumberField label="Mensualités des crédits en cours" suffix="€" hint="Vide si aucune." {...bind('debts')} />
        <NumberField
          label="Taux d’endettement maximal"
          suffix="%"
          hint={`La recommandation du HCSF est de ${pct(HCSF_MAX_DEBT_RATIO_PCT, 0)}.`}
          {...bind('ratio')}
        />
        <NumberField label="Taux nominal annuel" suffix="%" {...bind('rate')} />
        <NumberField label="Taux d’assurance annuel" suffix="%" hint="Vide si aucune." {...bind('insurance')} />
        <NumberField label="Durée" suffix="ans" {...bind('years')} />
      </div>
      <Results
        ready={result !== null}
        empty="Renseignez vos revenus, le taux d’endettement maximal, le taux et la durée (50 ans au plus)."
      >
        {result ? (
          <>
            <Stat label="Taux d’endettement actuel" value={pct(result.currentDebtRatioPct, 1)} />
            <Stat
              label="Mensualité disponible"
              value={money(result.maxMonthlyPayment, { decimals: 2 })}
              sub={result.monthlyInsurance > 0 ? `dont ${money(result.monthlyInsurance, { decimals: 2 })} d’assurance` : undefined}
            />
            <Stat label="Capacité d’emprunt" value={money(result.capacity)} sub={result.capacity > 0 ? undefined : 'plafond déjà atteint'} />
            <Stat label="Taux d’endettement après emprunt" value={pct(result.resultingDebtRatioPct, 1)} />
          </>
        ) : null}
      </Results>
    </Panel>
  )
}

const BUY_KEYS = [
  'price', 'fees', 'downPayment', 'rate', 'insurance', 'years', 'ownerCosts',
  'rent', 'rentIncrease', 'appreciation', 'investReturn', 'horizon', 'saleFees',
] as const

function BuyVsRentSimulator() {
  const { values, bind, fill } = useForm(BUY_KEYS)
  const read = readAll({
    price: readField(values.price),
    fees: readField(values.fees, { optional: true }),
    downPayment: readField(values.downPayment, { optional: true }),
    rate: readField(values.rate, { optional: true }),
    insurance: readField(values.insurance, { optional: true }),
    years: readField(values.years, { optional: true, max: MAX_YEARS }),
    ownerCosts: readField(values.ownerCosts, { optional: true }),
    rent: readField(values.rent),
    rentIncrease: readField(values.rentIncrease, { optional: true, signed: true }),
    appreciation: readField(values.appreciation, { optional: true, signed: true }),
    investReturn: readField(values.investReturn, { optional: true, signed: true }),
    horizon: readField(values.horizon, { max: MAX_YEARS }),
    saleFees: readField(values.saleFees, { optional: true, max: 100 }),
  })
  const needsLoan = read !== null && read.price + read.fees > read.downPayment
  const ready = read !== null && read.price > 0 && read.horizon >= 1 && (!needsLoan || read.years > 0)
  const result =
    ready && read
      ? compareBuyVsRent({
          price: read.price,
          acquisitionFees: read.fees,
          downPayment: read.downPayment,
          loanRatePct: read.rate,
          insuranceRatePct: read.insurance,
          loanMonths: Math.round(read.years * 12),
          ownerYearlyCosts: read.ownerCosts,
          appreciationPct: read.appreciation,
          monthlyRent: read.rent,
          rentIncreasePct: read.rentIncrease,
          investmentReturnPct: read.investReturn,
          horizonYears: Math.round(read.horizon),
          saleFeesPct: read.saleFees,
        })
      : null

  return (
    <Panel title="Acheter ou louer">
      <p className="mb-4 max-w-[68ch] text-muted">
        À budget mensuel égal : le locataire place l’apport et l’écart avec le coût du propriétaire, ou l’inverse
        si le loyer coûte plus cher. Patrimoine comparé à la fin de l’horizon, avant impôts.
      </p>
      <PropertyPicker
        onPick={(item) => {
          const prefill = prefillFromProperty(item)
          fill({
            price: prefill.price,
            fees: prefill.fees,
            downPayment: prefill.downPayment,
            rate: prefill.rate,
            insurance: prefill.insurance,
            years: prefill.years,
            ownerCosts: toInput(item.property.monthly_charges * 12 + item.property.yearly_property_tax),
          })
        }}
      />
      <div className="flex flex-col gap-5">
        <FieldGroup legend="Achat">
          <NumberField label="Prix d’achat" suffix="€" {...bind('price')} />
          <NumberField label="Frais d’acquisition" suffix="€" {...bind('fees')} />
          <NumberField label="Apport personnel" suffix="€" {...bind('downPayment')} />
          <NumberField label="Charges annuelles du propriétaire" suffix="€" hint="Taxe foncière, copropriété, entretien." {...bind('ownerCosts')} />
          <NumberField label="Taux nominal annuel" suffix="%" {...bind('rate')} />
          <NumberField label="Taux d’assurance annuel" suffix="%" {...bind('insurance')} />
          <NumberField label="Durée du prêt" suffix="ans" {...bind('years')} />
          <NumberField label="Revalorisation annuelle du bien" suffix="%" {...bind('appreciation')} />
        </FieldGroup>
        <FieldGroup legend="Location et placements">
          <NumberField label="Loyer mensuel d’un logement équivalent" suffix="€" {...bind('rent')} />
          <NumberField label="Hausse annuelle des loyers et charges" suffix="%" {...bind('rentIncrease')} />
          <NumberField label="Rendement annuel des placements" suffix="%" {...bind('investReturn')} />
        </FieldGroup>
        <FieldGroup legend="Horizon">
          <NumberField label="Horizon de comparaison" suffix="ans" {...bind('horizon')} />
          <NumberField label="Frais de revente" suffix="%" {...bind('saleFees')} />
        </FieldGroup>
      </div>
      <Results
        ready={result !== null}
        empty="Renseignez le prix, le loyer équivalent, l’horizon et, si vous empruntez, la durée du prêt (50 ans au plus)."
      >
        {result ? (
          <>
            <Stat label="Patrimoine en achetant" value={money(result.buyerWealth)} sub="bien revendu, dette soldée" />
            <Stat label="Patrimoine en louant" value={money(result.renterWealth)} sub="placements accumulés" />
            <Stat
              label={result.advantage >= 0 ? 'Avantage à l’achat' : 'Avantage à la location'}
              value={money(Math.abs(result.advantage))}
              tone={result.advantage >= 0 ? 'pos' : undefined}
            />
            <Stat
              label="Achat rentabilisé"
              value={result.breakEvenYear === null ? 'Pas sur l’horizon' : `Après ${duration(result.breakEvenYear * 12)}`}
            />
          </>
        ) : null}
      </Results>
    </Panel>
  )
}

function LoanSimulator() {
  const { values, bind } = useForm(['amount', 'rate', 'insurance', 'years'] as const)
  const read = readAll({
    amount: readField(values.amount),
    rate: readField(values.rate),
    insurance: readField(values.insurance, { optional: true }),
    years: readField(values.years, { max: MAX_YEARS }),
  })
  const ready = read !== null && read.amount > 0 && read.years > 0
  const months = ready && read ? Math.round(read.years * 12) : 0
  const summary = ready && read ? loanSummary(read.amount, read.rate, read.insurance, months) : null

  return (
    <Panel title="Simulateur de prêt">
      <p className="mb-4 max-w-[68ch] text-muted">
        Estimez la mensualité et le coût d’un emprunt à taux fixe avant de l’ajouter. L’assurance est calculée sur le
        capital initial.
      </p>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <NumberField label="Montant emprunté" suffix="€" {...bind('amount')} />
        <NumberField label="Taux nominal annuel" suffix="%" {...bind('rate')} />
        <NumberField label="Taux d’assurance annuel" suffix="%" hint="Vide si aucune." {...bind('insurance')} />
        <NumberField label="Durée" suffix="ans" {...bind('years')} />
      </div>
      <Results ready={summary !== null} empty="Renseignez le montant, le taux et la durée (50 ans au plus) pour afficher la mensualité.">
        {summary ? (
          <>
            <Stat
              label="Mensualité"
              value={money(summary.monthlyTotal, { decimals: 2 })}
              sub={
                summary.monthlyInsurance > 0
                  ? `dont ${money(summary.monthlyInsurance, { decimals: 2 })} d’assurance`
                  : `sur ${duration(months)}`
              }
            />
            <Stat label="Intérêts" value={money(summary.totalInterest)} />
            <Stat label="Assurance" value={money(summary.totalInsurance)} />
            <Stat
              label="Coût total du crédit"
              value={money(summary.totalCost)}
              sub={`${money(summary.totalRepaid)} remboursés au total`}
            />
          </>
        ) : null}
      </Results>
    </Panel>
  )
}

type SimulatorKey = 'rental' | 'borrowing' | 'buy_vs_rent' | 'loan'

export function Simulators() {
  const [current, setCurrent] = useState<SimulatorKey>('rental')
  return (
    <div className="flex flex-col gap-4">
      <div className="overflow-x-auto">
        <Segmented
          label="Simulateur"
          value={current}
          onChange={setCurrent}
          options={[
            { value: 'rental', label: 'Investissement locatif' },
            { value: 'borrowing', label: 'Capacité d’emprunt' },
            { value: 'buy_vs_rent', label: 'Acheter ou louer' },
            { value: 'loan', label: 'Prêt' },
          ]}
        />
      </div>
      {current === 'rental' ? <RentalSimulator /> : null}
      {current === 'borrowing' ? <BorrowingSimulator /> : null}
      {current === 'buy_vs_rent' ? <BuyVsRentSimulator /> : null}
      {current === 'loan' ? <LoanSimulator /> : null}
    </div>
  )
}
