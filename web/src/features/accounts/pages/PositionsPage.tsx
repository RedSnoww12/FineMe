import { Link } from 'react-router'
import { money, number, pct } from '@/shared/lib/format'
import { useView } from '@/shared/state/view'
import { EmptyState, ErrorState, LoadingBlock, PageHeader, Panel, Signed } from '@/shared/ui/ui'
import { useAllAccountDetails } from '../hooks/useAccountsApi'
import { consolidate } from '../lib/consolidation'

const backToAccounts = (
  <Link to="/comptes" className="mb-1 inline-flex text-[13.5px] text-muted hover:text-ink">
    ← Comptes
  </Link>
)

/** Une ligne par actif (ISIN, symbole ou nom), tous comptes confondus. */
export default function PositionsPage() {
  const { memberId } = useView()
  const details = useAllAccountDetails()
  const header = (
    <PageHeader
      title="Positions consolidées"
      description="Chaque titre, fonds ou crypto sur une seule ligne, quel que soit le nombre de comptes qui le détiennent."
      back={backToAccounts}
    />
  )
  if (details.isPending) return <>{header}<LoadingBlock rows={4} /></>
  if (details.error || !details.data) return <>{header}<ErrorState error={details.error} onRetry={details.refetch} /></>

  const assets = consolidate(details.data, memberId)
  if (assets.length === 0) {
    return (
      <>
        {header}
        <EmptyState title="Aucune position">Ajoutez vos titres, fonds ou cryptos depuis la fiche d’un compte.</EmptyState>
      </>
    )
  }
  return (
    <>
      {header}
      <Panel flush>
        <div className="table-wrap">
          <table className="table">
            <caption className="sr-only">Positions consolidées</caption>
            <thead>
              <tr>
                <th scope="col">Actif</th>
                <th scope="col" className="r">Comptes</th>
                <th scope="col" className="r">Quantité</th>
                <th scope="col" className="r">Valeur</th>
                <th scope="col" className="r">Plus-value</th>
              </tr>
            </thead>
            <tbody>
              {assets.map((asset) => (
                <tr key={asset.key}>
                  <td className="min-w-[180px]">
                    <Link to={`/comptes/positions/${encodeURIComponent(asset.key)}`} className="font-medium text-accent hover:underline">
                      {asset.name}
                    </Link>
                    <div className="num text-[13px] text-muted">{[asset.symbol, asset.isin].filter(Boolean).join(', ') || '–'}</div>
                  </td>
                  <td className="r">{asset.lines.length}</td>
                  <td className="r num">{number(asset.quantity, 6)}</td>
                  <td className="r num font-medium">{money(asset.value)}</td>
                  <td className="r num">
                    <Signed value={asset.gain}>{money(asset.gain, { signed: true })}</Signed>
                    {asset.gainPct !== null ? (
                      <div className="text-[12.5px]">
                        <Signed value={asset.gainPct}>{pct(asset.gainPct, 1, true)}</Signed>
                      </div>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Panel>
    </>
  )
}
