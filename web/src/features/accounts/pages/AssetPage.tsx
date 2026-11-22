import { Link, useParams } from 'react-router'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { AXIS_TICK, MoneyTooltip } from '@/shared/ui/charts'
import { date, money, number, pct } from '@/shared/lib/format'
import { useView } from '@/shared/state/view'
import { EmptyState, ErrorState, LoadingBlock, PageHeader, Panel, Signed, Skeleton, Stat } from '@/shared/ui/ui'
import { useAllAccountDetails, useHoldingPrices, useOperationsOf } from '../hooks/useAccountsApi'
import { consolidate, type Asset } from '../lib/consolidation'
import { OPERATION_KIND_LABEL } from '../lib/operations'

const back = (
  <Link to="/comptes/positions" className="mb-1 inline-flex text-[13.5px] text-muted hover:text-ink">
    ← Positions consolidées
  </Link>
)

function PricePanel({ asset }: { asset: Asset }) {
  const prices = useHoldingPrices(asset.lines[0]?.holding.id)
  const points = prices.data?.points ?? []
  return (
    <Panel title="Historique du cours">
      {prices.isPending ? (
        <Skeleton className="h-[220px]" />
      ) : prices.isError ? (
        <ErrorState error={prices.error} onRetry={() => void prices.refetch()} />
      ) : points.length < 2 ? (
        <p className="text-muted">Le cours se relève chaque jour pour les positions à cours automatique : la courbe apparaîtra ensuite.</p>
      ) : (
        <div role="img" aria-label={`Cours de ${asset.name}, du ${date(points[0]?.date)} au ${date(points.at(-1)?.date)}.`} className="h-[220px]">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={points} accessibilityLayer={false}>
              <CartesianGrid vertical={false} stroke="var(--line)" />
              <XAxis dataKey="date" tick={AXIS_TICK} tickLine={false} axisLine={false} tickFormatter={(v: string) => date(v, 'month')} minTickGap={32} />
              <YAxis tick={AXIS_TICK} tickLine={false} axisLine={false} width={56} domain={['auto', 'auto']} />
              <Tooltip content={<MoneyTooltip formatLabel={(v) => date(String(v), 'long')} />} />
              <Line dataKey="price" name="Cours" stroke="var(--c1)" dot={false} strokeWidth={2} isAnimationActive={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </Panel>
  )
}

function OperationsPanel({ asset }: { asset: Asset }) {
  const operations = useOperationsOf([...new Set(asset.lines.map((line) => line.accountId))])
  const holdingIds = new Set(asset.lines.map((line) => line.holding.id))
  const accountName = new Map(asset.lines.map((line) => [line.accountId, line.accountName]))
  const rows = (operations.data ?? [])
    .filter((op) => op.holding_id !== null && holdingIds.has(op.holding_id))
    .sort((a, b) => b.date.localeCompare(a.date))
  return (
    <Panel title="Opérations, tous comptes confondus" flush>
      {operations.isPending ? (
        <div className="px-4 pb-4 sm:px-5"><Skeleton className="h-[120px]" /></div>
      ) : operations.error ? (
        <div className="px-4 pb-4 sm:px-5"><ErrorState error={operations.error} /></div>
      ) : rows.length === 0 ? (
        <p className="px-4 pb-4 text-muted sm:px-5">Aucune opération enregistrée pour cet actif.</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <caption className="sr-only">Opérations de {asset.name}</caption>
            <thead>
              <tr>
                <th scope="col">Date</th>
                <th scope="col">Type</th>
                <th scope="col">Compte</th>
                <th scope="col" className="r">Quantité</th>
                <th scope="col" className="r">Montant</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((op) => (
                <tr key={op.id}>
                  <td>{date(op.date)}</td>
                  <td>{OPERATION_KIND_LABEL[op.kind]}</td>
                  <td>{accountName.get(op.account_id)}</td>
                  <td className="r num">{op.quantity === null ? '–' : number(op.quantity, 6)}</td>
                  <td className="r num">{money(op.amount, { decimals: 2, currency: op.currency })}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  )
}

function Split({ caption, rows, total }: { caption: string; rows: { key: string; label: string; value: number; sub?: string }[]; total: number }) {
  return (
    <div className="table-wrap">
      <table className="table">
        <caption className="sr-only">{caption}</caption>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key}>
              <th scope="row" className="font-normal">
                {row.label}
                {row.sub ? <div className="num text-[13px] text-muted">{row.sub}</div> : null}
              </th>
              <td className="r num">{money(row.value)}</td>
              <td className="r num text-muted">{total > 0 ? pct((row.value / total) * 100, 0) : '–'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function AssetPage() {
  const { key = '' } = useParams()
  const { memberId } = useView()
  const details = useAllAccountDetails()
  const asset = details.data ? consolidate(details.data, memberId).find((a) => a.key === key) : undefined

  if (details.isPending) return <><PageHeader title="Position" back={back} /><LoadingBlock rows={4} /></>
  if (details.error || !details.data) {
    return <><PageHeader title="Position" back={back} /><ErrorState error={details.error} onRetry={details.refetch} /></>
  }
  if (!asset) {
    return (
      <>
        <PageHeader title="Position" back={back} />
        <EmptyState title="Position introuvable">Cet actif n’est détenu dans aucun compte de la vue affichée.</EmptyState>
      </>
    )
  }

  const total = asset.value
  return (
    <>
      <PageHeader title={asset.name} description={[asset.symbol, asset.isin].filter(Boolean).join(' · ') || undefined} back={back} />
      <div className="flex flex-col gap-4 sm:gap-5">
        <Panel>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <Stat label="Quantité totale" value={number(asset.quantity, 6)} />
            <Stat label="Valeur" value={money(asset.value)} />
            <Stat
              label="PRU moyen pondéré"
              value={money(asset.averageBuyPrice, { decimals: 2, currency: asset.currency })}
              sub={asset.averageBuyPrice === null && asset.lines.length > 1 ? 'PRU inconnu ou devises différentes' : undefined}
            />
            <Stat
              label="Plus-value"
              value={<Signed value={asset.gain}>{money(asset.gain, { signed: true })}</Signed>}
              sub={asset.gainPct !== null ? pct(asset.gainPct, 1, true) : undefined}
            />
          </div>
        </Panel>
        <div className="grid gap-4 sm:gap-5 lg:grid-cols-2">
          <Panel title="Par compte">
            <Split
              caption="Répartition par compte"
              total={total}
              rows={asset.lines.map((line) => ({
                key: line.holding.id,
                label: line.accountName,
                value: line.value,
                sub: `${number(line.quantity, 6)} × PRU ${money(line.holding.buy_price, { decimals: 2, currency: line.holding.currency })}`,
              }))}
            />
          </Panel>
          <Panel title="Par membre">
            <Split caption="Répartition par membre" total={total} rows={asset.members.map((m) => ({ key: m.memberId, label: m.name, value: m.value }))} />
          </Panel>
        </div>
        <PricePanel asset={asset} />
        <OperationsPanel asset={asset} />
      </div>
    </>
  )
}
