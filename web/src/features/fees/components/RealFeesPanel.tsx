import { useState } from 'react'
import { useAllAccountDetails } from '@/shared/api/queries'
import { money, pct, toInput } from '@/shared/lib/format'
import { parseFrNumber } from '@/shared/lib/parse'
import { useView } from '@/shared/state/view'
import { Badge, Button, ErrorState, NumberField, Panel, Skeleton, Stat } from '@/shared/ui/ui'
import {
  cumulativeCost,
  DEFAULT_GROSS_RETURN_PCT,
  DEFAULT_THRESHOLD_PCT,
  portfolioFees,
  type AccountCost,
} from '../lib/portfolioFees'

const HORIZONS = [10, 20, 30]

/** Coût annuel réel des comptes de titres : chaque frais rattaché à son enveloppe ou à son fonds. */
export function RealFeesPanel({ onTest }: { onTest: (account: AccountCost) => void }) {
  const { memberId } = useView()
  const details = useAllAccountDetails()
  const [grossReturn, setGrossReturn] = useState(toInput(DEFAULT_GROSS_RETURN_PCT))
  const [threshold, setThreshold] = useState(toInput(DEFAULT_THRESHOLD_PCT))

  if (details.isPending) return <Panel title="Vos frais réels"><Skeleton className="h-[160px]" /></Panel>
  if (details.error || !details.data) {
    return <Panel title="Vos frais réels"><ErrorState error={details.error} onRetry={details.refetch} /></Panel>
  }

  const fees = portfolioFees(details.data, memberId)
  if (fees.accounts.length === 0) {
    return (
      <Panel title="Vos frais réels">
        <p className="text-muted">Aucun compte de titres ou de fonds : rien à analyser.</p>
      </Panel>
    )
  }
  const limit = parseFrNumber(threshold) ?? DEFAULT_THRESHOLD_PCT
  const returnPct = parseFrNumber(grossReturn) ?? DEFAULT_GROSS_RETURN_PCT

  return (
    <Panel title="Vos frais réels">
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Stat label="Coût annuel total" value={money(fees.totalCost)} sub={`${pct(fees.value > 0 ? (fees.totalCost / fees.value) * 100 : 0, 2)} de l’encours`} />
        <Stat label="Frais des enveloppes" value={money(fees.envelopeCost)} />
        <Stat label="Frais courants des fonds" value={money(fees.fundsCost)} />
        <NumberField label="Rendement brut supposé" value={grossReturn} onChange={setGrossReturn} suffix="%" />
      </div>
      <p className="mt-3 text-[14px] text-muted">
        Coût cumulé des frais actuels, rendement perdu compris, sans nouveau versement :{' '}
        {HORIZONS.map((years, i) => (
          <span key={years}>
            {i > 0 ? ', ' : ''}
            <span className="num text-ink">{money(cumulativeCost(fees, years, returnPct))}</span> sur {years} ans
          </span>
        ))}
        .
      </p>
      {fees.unknownFunds > 0 ? (
        <p className="mt-2 text-[13.5px] text-muted">
          {fees.unknownFunds === 1 ? 'Une position n’a' : `${fees.unknownFunds} positions n’ont`} pas de frais courants
          renseignés : le coût réel peut être plus élevé.
        </p>
      ) : null}

      <div className="table-wrap mt-4">
        <table className="table">
          <caption className="sr-only">Frais annuels par compte</caption>
          <thead>
            <tr>
              <th scope="col">Compte</th>
              <th scope="col" className="r">Valeur</th>
              <th scope="col" className="r">Enveloppe</th>
              <th scope="col" className="r">Fonds</th>
              <th scope="col" className="r">Total par an</th>
              <th scope="col"><span className="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            {fees.accounts.map((account) => (
              <tr key={account.accountId}>
                <th scope="row" className="font-normal">{account.name}</th>
                <td className="r num">{money(account.value)}</td>
                <td className="r num">
                  {account.envelopePct === null ? (
                    <span className="text-muted">non renseignés</span>
                  ) : (
                    <>
                      {money(account.envelopeCost)}
                      <div className="text-[12.5px] text-muted">{pct(account.envelopePct, 2)}</div>
                    </>
                  )}
                </td>
                <td className="r num">{money(account.fundsCost)}</td>
                <td className="r num font-medium">
                  {money(account.totalCost)}
                  <div className="text-[12.5px] font-normal text-muted">{pct(account.effectivePct, 2)}</div>
                </td>
                <td className="r">
                  <Button size="sm" variant="ghost" onClick={() => onTest(account)}>
                    Tester une alternative
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="mt-5 flex flex-wrap items-end justify-between gap-3">
        <h3 className="font-semibold">Positions les plus coûteuses</h3>
        <NumberField label="Seuil de frais courants" value={threshold} onChange={setThreshold} suffix="%" fieldClassName="w-44" />
      </div>
      {fees.funds.length === 0 ? (
        <p className="mt-2 text-muted">Aucune position avec des frais courants renseignés.</p>
      ) : (
        <div className="table-wrap mt-2">
          <table className="table">
            <caption className="sr-only">Positions classées par coût annuel</caption>
            <thead>
              <tr>
                <th scope="col">Position</th>
                <th scope="col" className="r">Frais courants</th>
                <th scope="col" className="r">Coût par an</th>
              </tr>
            </thead>
            <tbody>
              {fees.funds.map((fund) => (
                <tr key={fund.holdingId}>
                  <th scope="row" className="font-normal">
                    {fund.name}
                    <div className="text-[13px] text-muted">{fund.accountName}</div>
                  </th>
                  <td className="r num">
                    {fund.ratePct > limit ? <Badge tone="neg">Au-dessus du seuil</Badge> : null} {pct(fund.ratePct, 2)}
                  </td>
                  <td className="r num">{money(fund.annualCost)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="mt-3 text-[13.5px] text-muted">
        Ces chiffres montrent des écarts de coût, pas une recommandation de produit.
      </p>
    </Panel>
  )
}
