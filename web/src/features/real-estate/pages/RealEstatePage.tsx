import { useState } from 'react'
import { useSearchParams } from 'react-router'
import { useAccounts, useDeleteAccount } from '@/shared/api/queries'
import { useRealEstate } from '../hooks/useRealEstateApi'
import type { RealEstateItem } from '@/shared/api/types'
import { AccountEditor } from '../components/AccountEditor'
import { ScheduleModal } from '../components/ScheduleModal'
import { Simulators } from '../components/Simulators'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  IconButton,
  LoadingBlock,
  PageHeader,
  Panel,
  Segmented,
  Signed,
  Stat,
} from '@/shared/ui/ui'
import { date, money, pct } from '@/shared/lib/format'
import { BUILDING_TYPE_LABEL, PROPERTY_CATEGORY_LABEL } from '@/shared/lib/labels'

type FormTarget =
  | { mode: 'property' }
  | { mode: 'loan'; propertyAccountId?: string }
  | { mode: 'edit'; accountId: string; kind: 'real_estate' | 'loan' }

function PropertyCard({
  item,
  onForm,
  onSchedule,
  onDelete,
}: {
  item: RealEstateItem
  onForm: (target: FormTarget) => void
  onSchedule: (accountId: string, name: string) => void
  onDelete: (accountId: string, name: string, isLoan: boolean) => void
}) {
  const { account, property } = item
  return (
    <section className="panel">
      <header className="flex flex-wrap items-start justify-between gap-3 px-4 pt-4 sm:px-5">
        <div className="min-w-0">
          <h2 className="text-[19px] font-semibold">{account.name}</h2>
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[13.5px] text-muted">
            <Badge tone="accent">{PROPERTY_CATEGORY_LABEL[property.category]}</Badge>
            <Badge>{BUILDING_TYPE_LABEL[property.building_type]}</Badge>
            {property.surface_m2 ? <span className="num">{property.surface_m2} m²</span> : null}
          </div>
          {property.address ? <p className="mt-1.5 text-[14px] text-muted">{property.address}</p> : null}
        </div>
        <div className="flex">
          <IconButton
            icon="edit"
            label={`Modifier ${account.name}`}
            onClick={() => onForm({ mode: 'edit', accountId: account.id, kind: 'real_estate' })}
          />
          <IconButton
            icon="trash"
            label={`Supprimer ${account.name}`}
            onClick={() => onDelete(account.id, account.name, false)}
          />
        </div>
      </header>

      <div className="grid grid-cols-2 gap-x-6 gap-y-5 px-4 py-5 sm:px-5 md:grid-cols-4">
        <Stat
          label="Valeur estimée"
          value={money(property.current_value)}
          sub={`acheté ${money(property.purchase_price)}${property.purchased_at ? ` en ${property.purchased_at.slice(0, 4)}` : ''}`}
        />
        <Stat label="Valeur nette de dette" value={money(item.equity)} />
        <Stat
          label="Plus-value latente"
          value={<Signed value={item.latent_gain}>{money(item.latent_gain, { signed: true })}</Signed>}
        />
        {property.monthly_rent > 0 ? (
          <Stat
            label="Rendement locatif"
            value={item.gross_yield_pct !== null ? `${pct(item.gross_yield_pct, 2)} brut` : '–'}
            sub={`${item.net_yield_pct !== null ? `${pct(item.net_yield_pct, 2)} net, ` : ''}loyer ${money(property.monthly_rent)} par mois`}
          />
        ) : (
          <Stat label="Location" value="Non loué" />
        )}
      </div>

      <div className="border-t border-line px-4 py-3 sm:px-5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-[14.5px] font-semibold">
            {item.loans.length === 0 ? 'Aucun prêt lié' : item.loans.length === 1 ? 'Prêt lié' : 'Prêts liés'}
          </h3>
          <Button size="sm" icon="plus" onClick={() => onForm({ mode: 'loan', propertyAccountId: account.id })}>
            Ajouter un prêt
          </Button>
        </div>
        {item.loans.length > 0 ? (
          <ul className="mt-2">
            {item.loans.map((loan) => (
              <li key={loan.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line py-2.5 last:border-0">
                <div className="min-w-[10rem] flex-1">
                  <div className="font-medium">{loan.name}</div>
                  <div className="num text-[13px] text-muted">
                    {pct(loan.interest_rate_pct, 2)}, {money(loan.monthly_payment + loan.monthly_insurance, { decimals: 2 })} par
                    mois, fin {date(loan.end_date, 'month')}
                  </div>
                </div>
                <div className="text-right">
                  <div className="num font-display text-[16px] font-semibold">{money(loan.remaining_capital)}</div>
                  <div className="text-[12.5px] text-muted">restant dû</div>
                </div>
                <div className="flex items-center">
                  <Button size="sm" icon="table" onClick={() => onSchedule(loan.account_id, loan.name)}>
                    Échéancier
                  </Button>
                  <IconButton
                    icon="edit"
                    label={`Modifier ${loan.name}`}
                    onClick={() => onForm({ mode: 'edit', accountId: loan.account_id, kind: 'loan' })}
                  />
                  <IconButton
                    icon="trash"
                    label={`Supprimer ${loan.name}`}
                    onClick={() => onDelete(loan.account_id, loan.name, true)}
                  />
                </div>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  )
}

export default function RealEstatePage() {
  const realEstate = useRealEstate()
  const accounts = useAccounts()
  const [form, setForm] = useState<FormTarget | null>(null)
  const [schedule, setSchedule] = useState<{ accountId: string; name: string } | null>(null)
  const [deleting, setDeleting] = useState<{ accountId: string; name: string; isLoan: boolean } | null>(null)
  const remove = useDeleteAccount(() => setDeleting(null))
  const [searchParams, setSearchParams] = useSearchParams()
  const tab: 'properties' | 'simulators' = searchParams.get('vue') === 'simulateurs' ? 'simulators' : 'properties'

  const properties = realEstate.data?.properties ?? []
  const linkedLoanIds = new Set(properties.flatMap((item) => item.loans.map((loan) => loan.account_id)))
  const otherLoans = (accounts.data ?? []).filter((account) => account.kind === 'loan' && !linkedLoanIds.has(account.id))
  const totals = realEstate.data?.totals

  const actions = (
    <>
      <Button icon="plus" onClick={() => setForm({ mode: 'loan' })}>
        Ajouter un prêt
      </Button>
      <Button variant="primary" icon="plus" onClick={() => setForm({ mode: 'property' })}>
        Ajouter un bien
      </Button>
    </>
  )
  const askDelete = (accountId: string, name: string, isLoan: boolean) => {
    remove.reset()
    setDeleting({ accountId, name, isLoan })
  }

  return (
    <>
      <PageHeader
        title="Immobilier"
        description="Vos biens, les prêts qui les financent et ce qu’il vous reste une fois la dette déduite."
        actions={realEstate.isSuccess && tab === 'properties' ? actions : undefined}
      />

      <div className="mb-4 sm:mb-5">
        <Segmented
          label="Section de l’immobilier"
          value={tab}
          onChange={(next) => setSearchParams(next === 'simulators' ? { vue: 'simulateurs' } : {}, { replace: true })}
          options={[
            { value: 'properties', label: 'Vos biens' },
            { value: 'simulators', label: 'Simulateurs' },
          ]}
        />
      </div>

      {tab === 'simulators' ? (
        <Simulators />
      ) : (
        <div className="flex flex-col gap-4 sm:gap-5">
          {realEstate.isPending ? (
            <LoadingBlock rows={3} />
          ) : realEstate.isError ? (
            <ErrorState error={realEstate.error} onRetry={() => void realEstate.refetch()} />
          ) : properties.length === 0 ? (
            <EmptyState title="Aucun bien immobilier">
              Ajoutez votre résidence principale ou un bien locatif avec sa valeur estimée, puis le prêt qui le finance :
              FineMe calcule le capital restant dû, la valeur nette et, pour un bien loué, les rendements.
            </EmptyState>
          ) : (
            <>
              {totals ? (
                <section className="panel grid grid-cols-2 gap-x-6 gap-y-5 px-4 py-5 sm:px-6 md:grid-cols-5">
                  <Stat label="Valeur des biens" value={money(totals.value)} />
                  <Stat label="Capital restant dû" value={money(totals.remaining_debt)} />
                  <Stat label="Valeur nette" value={money(totals.equity)} />
                  <Stat label="Loyers mensuels" value={money(totals.monthly_rent)} />
                  <Stat label="Mensualités" value={money(totals.monthly_payments)} />
                </section>
              ) : null}
              {properties.map((item) => (
                <PropertyCard
                  key={item.account.id}
                  item={item}
                  onForm={setForm}
                  onSchedule={(accountId, name) => setSchedule({ accountId, name })}
                  onDelete={askDelete}
                />
              ))}
            </>
          )}

          {otherLoans.length > 0 ? (
            <Panel title="Prêts sans bien lié" flush>
              <ul>
                {otherLoans.map((loan) => (
                  <li key={loan.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line px-4 py-2.5 last:border-0 sm:px-5">
                    <div className="min-w-[10rem] flex-1">
                      <div className="font-medium">{loan.name}</div>
                      {loan.institution ? <div className="text-[13px] text-muted">{loan.institution}</div> : null}
                    </div>
                    <div className="text-right">
                      <div className="num font-display text-[16px] font-semibold">{money(loan.owned_value)}</div>
                      <div className="text-[12.5px] text-muted">restant dû</div>
                    </div>
                    <div className="flex items-center">
                      <Button size="sm" icon="table" onClick={() => setSchedule({ accountId: loan.id, name: loan.name })}>
                        Échéancier
                      </Button>
                      <IconButton
                        icon="edit"
                        label={`Modifier ${loan.name}`}
                        onClick={() => setForm({ mode: 'edit', accountId: loan.id, kind: 'loan' })}
                      />
                      <IconButton icon="trash" label={`Supprimer ${loan.name}`} onClick={() => askDelete(loan.id, loan.name, true)} />
                    </div>
                  </li>
                ))}
              </ul>
            </Panel>
          ) : null}
        </div>
      )}

      {form?.mode === 'property' ? <AccountEditor kinds={['real_estate']} onClose={() => setForm(null)} /> : null}
      {form?.mode === 'loan' ? (
        <AccountEditor kinds={['loan']} propertyAccountId={form.propertyAccountId} onClose={() => setForm(null)} />
      ) : null}
      {form?.mode === 'edit' ? (
        <AccountEditor accountId={form.accountId} kinds={[form.kind]} onClose={() => setForm(null)} />
      ) : null}
      {schedule ? (
        <ScheduleModal accountId={schedule.accountId} name={schedule.name} onClose={() => setSchedule(null)} />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title={deleting.isLoan ? 'Supprimer ce prêt ?' : 'Supprimer ce bien ?'}
          confirmLabel={deleting.isLoan ? 'Supprimer le prêt' : 'Supprimer le bien'}
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.accountId)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.name} » sera supprimé définitivement.
        </ConfirmModal>
      ) : null}
    </>
  )
}
