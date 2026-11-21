import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'
import { ApiError } from '@/shared/api/client'
import { useAccount, useDeleteAccount } from '@/shared/api/queries'
import { useDeleteHolding, useDeleteTransaction, useRefreshPrices, useTransactions } from '../hooks/useAccountsApi'
import type { AccountDetail, Holding, PricesRefresh, Transaction } from '@/shared/api/types'
import { AccountEditor } from '../components/AccountEditor'
import { CsvImportModal } from '../components/CsvImport'
import { HoldingFormModal } from '../components/HoldingForm'
import { OperationsJournal } from '../components/OperationsJournal'
import { Icon } from '@/shared/ui/Icon'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Notice,
  PageHeader,
  Panel,
  Segmented,
  Signed,
  Stat,
} from '@/shared/ui/ui'
import { date, dateTime, money, number, pct, pctFromFraction } from '@/shared/lib/format'
import {
  ACCOUNT_KIND_LABEL,
  BUILDING_TYPE_LABEL,
  envelopeLabel,
  PROPERTY_CATEGORY_LABEL,
} from '@/shared/lib/labels'

const PAGE_SIZE = 25

const SOURCE_LABEL: Record<AccountDetail['source'], string> = {
  manual: 'Saisie manuelle',
  csv: 'Import CSV',
  aggregator: 'Connexion bancaire',
  import: 'Import complet',
}

const BackLink = () => (
  <Link to="/comptes" className="mb-2 inline-flex items-center gap-1 text-[14px] text-muted hover:text-ink">
    <Icon name="back" size={15} />
    Comptes
  </Link>
)

function HoldingsPanel({ account, onImport }: { account: AccountDetail; onImport: () => void }) {
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<Holding | null>(null)
  const [deleting, setDeleting] = useState<Holding | null>(null)
  const [refreshed, setRefreshed] = useState<PricesRefresh | null>(null)
  const remove = useDeleteHolding(() => setDeleting(null))
  const refresh = useRefreshPrices(setRefreshed)
  const holdings = account.holdings
  const staleCount = holdings.filter((holding) => holding.price_stale).length

  return (
    <Panel
      title="Positions"
      flush
      action={
        <div className="flex flex-wrap gap-2">
          {holdings.length > 0 ? (
            <Button
              size="sm"
              icon="refresh"
              busy={refresh.isPending}
              onClick={() => {
                setRefreshed(null)
                refresh.mutate(undefined)
              }}
            >
              Actualiser les cours
            </Button>
          ) : null}
          <Button size="sm" icon="upload" onClick={onImport}>
            Importer un CSV
          </Button>
          <Button size="sm" variant="primary" icon="plus" onClick={() => setAdding(true)}>
            Ajouter une position
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3 px-4 pt-2 empty:hidden sm:px-5">
        {refresh.isError ? <FormError error={refresh.error} /> : null}
        {refreshed ? (
          <Notice>
            {refreshed.updated > 0
              ? `${refreshed.updated} cours mis à jour.`
              : 'Aucun cours n’a pu être mis à jour automatiquement.'}
            {refreshed.failed.length > 0 ? (
              <>
                {' '}
                Sans cotation trouvée, ces positions gardent leur prix saisi :{' '}
                {refreshed.failed.map((failure) => `${failure.name} (${failure.reason})`).join(', ')}.
              </>
            ) : null}
          </Notice>
        ) : null}
        {staleCount > 0 ? (
          <Notice>
            {staleCount === 1
              ? 'Un cours n’est plus mis à jour automatiquement depuis plusieurs jours : il est signalé « ancien ».'
              : `${staleCount} cours ne sont plus mis à jour automatiquement depuis plusieurs jours : ils sont signalés « ancien ».`}{' '}
            Actualisez les cours, ou vérifiez l’ISIN et le ticker de ces positions.
          </Notice>
        ) : null}
      </div>

      {holdings.length === 0 ? (
        <p className="px-4 py-5 text-muted sm:px-5">
          Aucune position. Ajoutez vos titres, fonds ou cryptos un par un, ou importez l’export CSV de votre courtier.
        </p>
      ) : (
        <div className="table-wrap mt-2">
          <table className="table">
            <thead>
              <tr>
                <th>Position</th>
                <th className="r">Quantité</th>
                <th className="r">Prix de revient</th>
                <th className="r">Cours</th>
                <th className="r">Valeur</th>
                <th className="r">Plus-value</th>
                <th className="r">Frais courants</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {holdings.map((holding) => (
                <tr key={holding.id}>
                  <td className="min-w-[180px]">
                    <div className="font-medium">{holding.name}</div>
                    <div className="num text-[13px] text-muted">
                      {[holding.symbol, holding.isin].filter(Boolean).join(', ') || '–'}
                    </div>
                  </td>
                  <td className="r">{number(holding.quantity, 6)}</td>
                  <td className="r">{money(holding.buy_price, { decimals: 2, currency: holding.currency })}</td>
                  <td className="r">
                    {money(holding.current_price, { decimals: 2, currency: holding.currency })}
                    <div className="text-[12.5px] text-muted">
                      {holding.price_source === 'auto'
                        ? holding.price_updated_at
                          ? date(holding.price_updated_at)
                          : 'auto'
                        : 'saisi'}
                      {holding.price_stale ? (
                        <>
                          {' · '}
                          <span className="font-medium text-neg">ancien</span>
                        </>
                      ) : null}
                    </div>
                  </td>
                  <td className="r font-medium">{money(holding.value)}</td>
                  <td className="r">
                    <Signed value={holding.gain}>{money(holding.gain, { signed: true })}</Signed>
                    {holding.gain_pct !== null ? (
                      <div className="text-[12.5px]">
                        <Signed value={holding.gain_pct}>{pct(holding.gain_pct, 1, true)}</Signed>
                      </div>
                    ) : null}
                  </td>
                  <td className="r">{pctFromFraction(holding.expense_ratio, 2)}</td>
                  <td>
                    <div className="flex justify-end">
                      <IconButton icon="edit" label={`Modifier ${holding.name}`} onClick={() => setEditing(holding)} />
                      <IconButton
                        icon="trash"
                        label={`Supprimer ${holding.name}`}
                        onClick={() => {
                          remove.reset()
                          setDeleting(holding)
                        }}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {adding || editing ? (
        <HoldingFormModal
          accountId={account.id}
          accountCurrency={account.currency}
          defaultKind={account.kind === 'crypto' ? 'crypto' : 'security'}
          holding={editing ?? undefined}
          onClose={() => {
            setAdding(false)
            setEditing(null)
          }}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer cette position ?"
          confirmLabel="Supprimer la position"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.name} » sera retirée de ce compte.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}

type InvestmentTab = 'holdings' | 'journal'

/** Positions et journal d'opérations, en onglets : le journal n'est chargé qu'à l'ouverture de son onglet. */
function InvestmentSection({ account, onImport }: { account: AccountDetail; onImport: () => void }) {
  const [tab, setTab] = useState<InvestmentTab>('holdings')
  return (
    <div className="flex flex-col gap-3">
      <Segmented
        label="Affichage des investissements"
        value={tab}
        onChange={setTab}
        options={[
          { value: 'holdings', label: 'Positions' },
          { value: 'journal', label: 'Journal d’opérations' },
        ]}
      />
      {tab === 'holdings' ? <HoldingsPanel account={account} onImport={onImport} /> : <OperationsJournal account={account} />}
    </div>
  )
}

function TransactionsPanel({ account, onImport }: { account: AccountDetail; onImport: () => void }) {
  const [offset, setOffset] = useState(0)
  const [deleting, setDeleting] = useState<Transaction | null>(null)
  const transactions = useTransactions(account.id, PAGE_SIZE, offset)
  const remove = useDeleteTransaction(() => setDeleting(null))
  const total = transactions.data?.total ?? 0
  const items = transactions.data?.items ?? []

  return (
    <Panel
      title="Opérations"
      flush
      action={
        <Button size="sm" icon="upload" onClick={onImport}>
          Importer un CSV
        </Button>
      }
    >
      {transactions.isPending ? (
        <div className="px-4 pb-4 sm:px-5">
          <LoadingBlock rows={2} />
        </div>
      ) : transactions.isError ? (
        <div className="px-4 pb-4 sm:px-5">
          <ErrorState error={transactions.error} onRetry={() => void transactions.refetch()} />
        </div>
      ) : items.length === 0 ? (
        <p className="px-4 py-5 text-muted sm:px-5">
          Aucune opération. Importez un relevé CSV depuis l’espace client de votre banque pour les voir ici.
        </p>
      ) : (
        <>
          <ul className="mt-2">
            {items.map((transaction) => (
              <li
                key={transaction.id}
                className="flex items-center gap-3 border-b border-line py-2.5 pr-2 pl-4 last:border-0 sm:pl-5"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate">{transaction.label}</div>
                  <div className="num text-[13px] text-muted">
                    {date(transaction.date)}
                    {transaction.category ? `, ${transaction.category}` : ''}
                  </div>
                </div>
                <Signed value={transaction.amount}>
                  <span className="font-medium">
                    {money(transaction.amount, { decimals: 2, currency: account.currency, signed: true })}
                  </span>
                </Signed>
                <IconButton
                  icon="trash"
                  label={`Supprimer l’opération ${transaction.label}`}
                  onClick={() => {
                    remove.reset()
                    setDeleting(transaction)
                  }}
                />
              </li>
            ))}
          </ul>
          {total > PAGE_SIZE ? (
            <div className="flex items-center justify-between gap-3 border-t border-line px-4 py-3 sm:px-5">
              <span className="num text-[13.5px] text-muted">
                {offset + 1} à {Math.min(offset + PAGE_SIZE, total)} sur {total}
              </span>
              <div className="flex gap-2">
                <Button size="sm" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>
                  Précédentes
                </Button>
                <Button size="sm" disabled={offset + PAGE_SIZE >= total} onClick={() => setOffset(offset + PAGE_SIZE)}>
                  Suivantes
                </Button>
              </div>
            </div>
          ) : null}
        </>
      )}
      {deleting ? (
        <ConfirmModal
          title="Supprimer cette opération ?"
          confirmLabel="Supprimer l’opération"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.label} » du {date(deleting.date)} sera supprimée. Le solde du compte n’est pas modifié.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}

export default function AccountDetailPage() {
  const { id } = useParams()
  const navigate = useNavigate()
  const account = useAccount(id)
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [importing, setImporting] = useState(false)
  const remove = useDeleteAccount(() => void navigate('/comptes'))

  if (account.isPending) {
    return (
      <>
        <PageHeader title="Compte" back={<BackLink />} />
        <LoadingBlock rows={3} />
      </>
    )
  }
  if (account.isError) {
    const missing = account.error instanceof ApiError && account.error.status === 404
    return (
      <>
        <PageHeader title="Compte" back={<BackLink />} />
        {missing ? (
          <EmptyState title="Compte introuvable">Ce compte a peut-être été supprimé.</EmptyState>
        ) : (
          <ErrorState error={account.error} onRetry={() => void account.refetch()} />
        )}
      </>
    )
  }

  const data = account.data
  const hasHoldings = data.kind === 'investment' || data.kind === 'crypto' || data.kind === 'savings' || data.kind === 'other' || data.holdings.length > 0
  const isPlain = data.kind !== 'real_estate' && data.kind !== 'loan'
  const partial = Math.abs(data.owned_value - data.value) > 0.005
  const gainPct = data.gain !== null && data.invested ? (data.gain / data.invested) * 100 : null

  return (
    <>
      <PageHeader
        title={data.name}
        back={<BackLink />}
        description={
          <span className="flex flex-wrap items-center gap-2">
            <Badge tone="accent">{ACCOUNT_KIND_LABEL[data.kind]}</Badge>
            {data.envelope ? <Badge>{envelopeLabel(data.envelope)}</Badge> : null}
            {data.institution ? <span>{data.institution}</span> : null}
          </span>
        }
        actions={
          <>
            <Button icon="edit" onClick={() => setEditing(true)}>
              Modifier
            </Button>
            <Button
              variant="danger"
              icon="trash"
              onClick={() => {
                remove.reset()
                setDeleting(true)
              }}
            >
              Supprimer
            </Button>
          </>
        }
      />

      <div className="flex flex-col gap-4 sm:gap-5">
        <section className="panel px-4 py-5 sm:px-6 sm:py-6">
          <p className="text-muted">{data.is_liability ? 'Capital restant dû' : partial ? 'Part détenue' : 'Valeur'}</p>
          <p className="num mt-1 font-display text-[36px] leading-none font-semibold tracking-tight sm:text-[44px]">
            {money(data.owned_value, { decimals: 2 })}
          </p>
          {partial ? <p className="num mt-2 text-muted">sur une valeur totale de {money(data.value, { decimals: 2 })}</p> : null}

          <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-5 md:grid-cols-4">
            {isPlain ? (
              <Stat label="Solde espèces" value={money(data.cash_balance, { decimals: 2, currency: data.currency })} />
            ) : null}
            {data.invested !== null ? <Stat label="Prix de revient" value={money(data.invested)} /> : null}
            {data.gain !== null ? (
              <Stat
                label="Plus-value latente"
                value={money(data.gain, { signed: true })}
                tone={data.gain > 0 ? 'pos' : data.gain < 0 ? 'neg' : undefined}
                sub={gainPct !== null ? pct(gainPct, 1, true) : undefined}
              />
            ) : null}
            {data.annual_yield_pct !== null ? <Stat label="Rendement annuel" value={pct(data.annual_yield_pct, 2)} /> : null}
          </div>

          <dl className="mt-6 grid gap-x-6 gap-y-3 border-t border-line pt-4 text-[14px] sm:grid-cols-3">
            <div>
              <dt className="text-muted">Propriétaires</dt>
              <dd>
                {data.owners.length === 0
                  ? 'Aucun'
                  : data.owners.map((owner) => `${owner.name} ${pctFromFraction(owner.share, 0)}`).join(', ')}
              </dd>
            </div>
            <div>
              <dt className="text-muted">Origine</dt>
              <dd>{SOURCE_LABEL[data.source] ?? data.source}</dd>
            </div>
            <div>
              <dt className="text-muted">Dernière mise à jour</dt>
              <dd className="num">{dateTime(data.updated_at)}</dd>
            </div>
          </dl>
        </section>

        {data.property ? (
          <Panel
            title="Bien immobilier"
            action={
              <Link to="/immobilier" className="text-[14px] font-medium text-accent hover:underline">
                Voir dans Immobilier
              </Link>
            }
          >
            <div className="grid grid-cols-2 gap-x-6 gap-y-5 md:grid-cols-4">
              <Stat label="Valeur estimée" value={money(data.property.current_value)} />
              <Stat
                label="Prix d’achat"
                value={money(data.property.purchase_price)}
                sub={data.property.purchase_fees > 0 ? `+ ${money(data.property.purchase_fees)} de frais` : undefined}
              />
              <Stat
                label="Type"
                value={BUILDING_TYPE_LABEL[data.property.building_type]}
                sub={PROPERTY_CATEGORY_LABEL[data.property.category]}
              />
              <Stat
                label="Surface"
                value={data.property.surface_m2 !== null ? `${number(data.property.surface_m2, 1)} m²` : '–'}
              />
            </div>
            {data.property.address ? <p className="mt-4 text-muted">{data.property.address}</p> : null}
          </Panel>
        ) : null}

        {data.loan ? (
          <Panel
            title="Prêt"
            action={
              <Link to="/immobilier" className="text-[14px] font-medium text-accent hover:underline">
                Voir le tableau d’amortissement
              </Link>
            }
          >
            <div className="grid grid-cols-2 gap-x-6 gap-y-5 md:grid-cols-4">
              <Stat label="Capital emprunté" value={money(data.loan.principal)} />
              <Stat
                label="Mensualité"
                value={money(data.loan.monthly_payment + data.loan.monthly_insurance, { decimals: 2 })}
                sub={
                  data.loan.monthly_insurance > 0
                    ? `dont ${money(data.loan.monthly_insurance, { decimals: 2 })} d’assurance`
                    : 'sans assurance'
                }
              />
              <Stat
                label="Taux"
                value={pct(data.loan.interest_rate_pct, 2)}
                sub={`${data.loan.duration_months} mois`}
              />
              <Stat label="Fin du prêt" value={date(data.loan.end_date, 'month')} sub={`début ${date(data.loan.start_date)}`} />
            </div>
          </Panel>
        ) : null}

        {isPlain && hasHoldings ? <InvestmentSection account={data} onImport={() => setImporting(true)} /> : null}
        {isPlain ? <TransactionsPanel account={data} onImport={() => setImporting(true)} /> : null}
      </div>

      {editing ? <AccountEditor accountId={data.id} kinds={[data.kind]} onClose={() => setEditing(false)} /> : null}
      {importing ? (
        <CsvImportModal
          accountId={data.id}
          currency={data.currency}
          allowHoldings={hasHoldings}
          onClose={() => setImporting(false)}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer ce compte ?"
          confirmLabel="Supprimer le compte"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(data.id)}
          onClose={() => setDeleting(false)}
        >
          « {data.name} », ses positions et ses opérations seront supprimés définitivement.
        </ConfirmModal>
      ) : null}
    </>
  )
}
