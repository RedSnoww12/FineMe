import type { ReactNode } from 'react'
import { Link, useSearchParams } from 'react-router'
import type { Account, Goal, HistoryPoint, Overview } from '@/shared/api/types'
import { date, money, pct, pctFromFraction, todayIso } from '@/shared/lib/format'
import { ACCOUNT_KIND_PLURAL, envelopeLabel, GOAL_STATUS } from '@/shared/lib/labels'
import { DISCREET_SHORTCUT, useDiscreet } from '@/shared/state/discreet'
import { useView } from '@/shared/state/view'
import { Icon } from '@/shared/ui/Icon'
import { Button, ErrorState, LoadingBlock, Notice, SelectField } from '@/shared/ui/ui'
import { useReportData, useScopeOptions } from '../hooks/useExport'
import { groupAccounts, summarizeHistory } from '../lib/report'
import { viewShare } from '../lib/tabular'

const HOUSEHOLD = 'foyer'

/** `?membre=foyer|<id>` ; absent : la vue affichée dans l'application. */
function useReportScope() {
  const [params, setParams] = useSearchParams()
  const { memberId } = useView()
  const param = params.get('membre')
  const scope = param === null ? memberId : param === HOUSEHOLD ? null : param
  const setScope = (id: string | null) => setParams({ membre: id ?? HOUSEHOLD }, { replace: true })
  return { scope, setScope }
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="report-block panel px-4 py-4 sm:px-6 print:px-0">
      <h2 className="mb-3 text-[17px] font-semibold">{title}</h2>
      {children}
    </section>
  )
}

function Summary({ overview }: { overview: Overview }) {
  const rows: [string, number][] = [
    ['Actifs bruts', overview.gross_assets],
    ['Dettes', overview.liabilities],
    ['Patrimoine net', overview.net_worth],
    ['dont actifs financiers', overview.financial_assets],
  ]
  return (
    <Section title="Synthèse">
      <dl className="grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt className="text-[13px] text-muted">{label}</dt>
            <dd className="num font-display text-[20px] font-semibold">{money(value)}</dd>
          </div>
        ))}
      </dl>
      {overview.invested > 0 ? (
        <p className="num mt-3 text-[13.5px] text-muted">
          Plus-value latente des positions au prix de revient connu : {money(overview.gain, { signed: true })} pour{' '}
          {money(overview.invested)} investis ({pct((overview.gain / overview.invested) * 100, 1, true)}).
        </p>
      ) : null}
    </Section>
  )
}

function Members({ overview }: { overview: Overview }) {
  if (overview.members.length < 2) return null
  return (
    <Section title="Par membre du foyer">
      <table className="table">
        <thead>
          <tr>
            <th>Membre</th>
            <th className="r">Actifs</th>
            <th className="r">Dettes</th>
            <th className="r">Net</th>
          </tr>
        </thead>
        <tbody>
          {overview.members.map((member) => (
            <tr key={member.member_id}>
              <td>{member.name}</td>
              <td className="r">{money(member.gross_assets)}</td>
              <td className="r">{money(member.liabilities)}</td>
              <td className="r">{money(member.net_worth)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Section>
  )
}

function Allocation({ overview }: { overview: Overview }) {
  return (
    <Section title="Répartition des actifs">
      {overview.allocation.length === 0 ? (
        <p className="text-muted">Aucun actif.</p>
      ) : (
        <div className="grid gap-6 sm:grid-cols-2 print:grid-cols-2">
          <table className="table">
            <thead>
              <tr>
                <th>Catégorie</th>
                <th className="r">Valeur</th>
                <th className="r">Part</th>
              </tr>
            </thead>
            <tbody>
              {overview.allocation.map((row) => (
                <tr key={row.category}>
                  <td>{row.label}</td>
                  <td className="r">{money(row.value)}</td>
                  <td className="r">{pct(row.pct)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {overview.by_envelope.length > 0 ? (
            <table className="table">
              <thead>
                <tr>
                  <th>Enveloppe</th>
                  <th className="r">Valeur</th>
                </tr>
              </thead>
              <tbody>
                {overview.by_envelope.map((row) => (
                  <tr key={row.envelope}>
                    <td>{envelopeLabel(row.envelope)}</td>
                    <td className="r">{money(row.value)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : null}
        </div>
      )}
    </Section>
  )
}

function Evolution({ points }: { points: HistoryPoint[] }) {
  const summary = summarizeHistory(points)
  return (
    <Section title="Évolution sur douze mois">
      {!summary ? (
        <p className="text-muted">Pas encore assez d’historique : il se construit jour après jour.</p>
      ) : (
        <>
          <p className="num mb-3">
            Patrimoine net de {money(summary.first.net_worth)} au {date(summary.first.date, 'long')} à{' '}
            {money(summary.last.net_worth)} au {date(summary.last.date, 'long')}, soit{' '}
            {money(summary.change, { signed: true })}
            {summary.changePct !== null ? ` (${pctFromFraction(summary.changePct, 1, true)})` : ''}.
          </p>
          <table className="table">
            <thead>
              <tr>
                <th>Fin de mois</th>
                <th className="r">Actifs</th>
                <th className="r">Dettes</th>
                <th className="r">Net</th>
              </tr>
            </thead>
            <tbody>
              {summary.monthly.map((point) => (
                <tr key={point.date}>
                  <td>{date(point.date)}</td>
                  <td className="r">{money(point.gross_assets)}</td>
                  <td className="r">{money(point.liabilities)}</td>
                  <td className="r">{money(point.net_worth)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </Section>
  )
}

function Goals({ goals }: { goals: Goal[] }) {
  return (
    <Section title="Objectifs">
      {goals.length === 0 ? (
        <p className="text-muted">Aucun objectif défini.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>Objectif</th>
              <th className="r">Atteint</th>
              <th className="r">Cible</th>
              <th className="r">Avancement</th>
              <th>Échéance</th>
              <th>État</th>
            </tr>
          </thead>
          <tbody>
            {goals.map((goal) => (
              <tr key={goal.id}>
                <td>{goal.name}</td>
                <td className="r">{money(goal.progress.current_amount)}</td>
                <td className="r">{money(goal.progress.target_amount)}</td>
                <td className="r">{pct(goal.progress.pct, 0)}</td>
                <td>{goal.target_date ? date(goal.target_date) : '–'}</td>
                <td>{GOAL_STATUS[goal.progress.status].label}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Section>
  )
}

function Accounts({ accounts, memberId }: { accounts: Account[]; memberId: string | null }) {
  const groups = groupAccounts(accounts, memberId)
  return (
    <Section title="Liste des comptes">
      {groups.length === 0 ? (
        <p className="text-muted">Aucun compte.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>Compte</th>
              <th>Établissement</th>
              <th className="r">Part</th>
              <th className="r">Valeur détenue</th>
            </tr>
          </thead>
          {groups.map((group) => (
            <tbody key={group.kind}>
              <tr>
                <th colSpan={3} scope="rowgroup" className="!text-ink">
                  {ACCOUNT_KIND_PLURAL[group.kind]}
                </th>
                <th className="r !text-ink">{money(group.total)}</th>
              </tr>
              {group.accounts.map((account) => (
                <tr key={account.id}>
                  <td>
                    {account.name}
                    {account.envelope ? <span className="text-muted"> · {envelopeLabel(account.envelope)}</span> : null}
                  </td>
                  <td>{account.institution ?? '–'}</td>
                  <td className="r">{pctFromFraction(viewShare(account.owners, memberId), 0)}</td>
                  <td className="r">{money(account.owned_value)}</td>
                </tr>
              ))}
            </tbody>
          ))}
        </table>
      )}
    </Section>
  )
}

export default function ReportPage() {
  const { scope, setScope } = useReportScope()
  const { options, labelOf } = useScopeOptions()
  const { hidden } = useDiscreet()
  const { overview, accounts, goals, history } = useReportData(scope)
  const queries = [overview, accounts, goals, history]
  const failed = queries.find((query) => query.isError)
  const data =
    overview.data && accounts.data && goals.data && history.data
      ? { overview: overview.data, accounts: accounts.data, goals: goals.data, points: history.data.points }
      : null
  const scopeLabel = labelOf(scope)

  return (
    <article aria-labelledby="report-title" className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3 print:hidden">
        <Link to="/parametres" className="inline-flex items-center gap-1 text-[14px] text-muted hover:text-ink">
          <Icon name="back" size={16} /> Paramètres
        </Link>
        <div className="flex flex-wrap items-end gap-3">
          <SelectField
            label="Périmètre"
            value={scope ?? ''}
            onChange={(value) => setScope(value || null)}
            options={options}
          />
          <Button variant="primary" icon="print" onClick={() => window.print()} disabled={!data}>
            Imprimer ou enregistrer en PDF
          </Button>
        </div>
      </div>
      {hidden ? (
        <div className="print:hidden">
          <Notice>
            Mode discret actif : les montants sont masqués dans le rapport. Désactivez-le (
            {DISCREET_SHORTCUT.replace('Shift', 'Maj')}) pour imprimer les montants.
          </Notice>
        </div>
      ) : null}

      <header className="report-block">
        <h1 id="report-title" className="text-[26px] font-semibold sm:text-[30px]">
          Rapport de patrimoine
        </h1>
        <p className="mt-1 text-muted">
          {scope ? `Part détenue par ${scopeLabel}` : 'Ensemble du foyer'} · établi le {date(todayIso(), 'long')} ·
          montants en euros
        </p>
      </header>

      {failed ? (
        <ErrorState error={failed.error} onRetry={() => queries.forEach((query) => void query.refetch())} />
      ) : !data ? (
        <LoadingBlock rows={4} label="Préparation du rapport…" />
      ) : (
        <>
          <Summary overview={data.overview} />
          {scope ? null : <Members overview={data.overview} />}
          <Allocation overview={data.overview} />
          <Evolution points={data.points} />
          <Goals goals={data.goals} />
          <Accounts accounts={data.accounts} memberId={scope} />
          <p className="text-[12.5px] text-muted">
            Valeurs détenues selon les parts de détention de chaque compte. Les dettes sont le capital restant dû des
            prêts. Les cours et soldes sont ceux connus de FineMe à la date du rapport.
          </p>
        </>
      )}
    </article>
  )
}
