import { useState, type FormEvent, type ReactNode } from 'react'
import { useAccounts, useGoals, useMembers } from '@/shared/api/queries'
import { useDeleteGoal, useSaveGoal } from '../hooks/useGoalsApi'
import type { Account, Goal, GoalInput, GoalKind, Member } from '@/shared/api/types'
import {
  Badge,
  Button,
  ConfirmModal,
  EmptyState,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Modal,
  NumberField,
  PageHeader,
  ProgressBar,
  Segmented,
  TextField,
} from '@/shared/ui/ui'
import { parseFrNumber } from '@/shared/lib/parse'
import { date, money, number, pct, toInput } from '@/shared/lib/format'
import { GOAL_STATUS } from '@/shared/lib/labels'

type FormState = {
  name: string
  icon: string
  kind: GoalKind
  target_amount: string
  target_date: string
  monthly_contribution: string
  expected_return_pct: string
  monthly_expenses: string
  coverage_months: string
  account_ids: string[]
  member_ids: string[]
}
type Errors = Partial<Record<keyof FormState, string>>

function CheckList({
  legend,
  hint,
  items,
  selected,
  onChange,
  empty,
}: {
  legend: string
  hint?: string
  items: { id: string; label: string; sub?: string }[]
  selected: string[]
  onChange: (ids: string[]) => void
  empty: string
}) {
  return (
    <fieldset className="rounded-[11px] border border-line p-3.5">
      <legend className="px-1 text-[13.5px] font-medium">{legend}</legend>
      {items.length === 0 ? (
        <p className="text-muted">{empty}</p>
      ) : (
        <ul className="flex max-h-52 flex-col overflow-y-auto">
          {items.map((item) => (
            <li key={item.id}>
              <label className="flex min-h-10 items-center gap-2.5 rounded-lg px-1 hover:bg-surface-2">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--accent)]"
                  checked={selected.includes(item.id)}
                  onChange={(event) =>
                    onChange(event.target.checked ? [...selected, item.id] : selected.filter((id) => id !== item.id))
                  }
                />
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.sub ? <span className="num text-[13px] text-muted">{item.sub}</span> : null}
              </label>
            </li>
          ))}
        </ul>
      )}
      {hint ? <p className="mt-2 text-[13px] text-muted">{hint}</p> : null}
    </fieldset>
  )
}

function GoalFormModal({
  goal,
  accounts,
  members,
  onClose,
}: {
  goal?: Goal
  accounts: Account[]
  members: Member[]
  onClose: () => void
}) {
  const [form, setForm] = useState<FormState>({
    name: goal?.name ?? '',
    icon: goal?.icon ?? '',
    kind: goal?.kind ?? 'project',
    target_amount: toInput(goal?.target_amount, 2),
    target_date: goal?.target_date ?? '',
    monthly_contribution: toInput(goal?.monthly_contribution, 2),
    expected_return_pct: toInput(goal?.expected_return_pct),
    monthly_expenses: toInput(goal?.monthly_expenses, 2),
    coverage_months: toInput(goal?.coverage_months),
    account_ids: goal?.account_ids ?? [],
    member_ids: goal?.member_ids ?? [],
  })
  const [errors, setErrors] = useState<Errors>({})
  const text = (key: keyof FormState) => (value: string) => setForm((f) => ({ ...f, [key]: value }))

  const mutation = useSaveGoal(goal?.id, onClose)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: Errors = {}
    const optional = (key: keyof FormState, min = 0): number | null => {
      const raw = String(form[key]).trim()
      if (!raw) return null
      const value = parseFrNumber(raw)
      if (value === null || value < min) {
        next[key] = 'Saisissez un nombre valide, ou laissez vide.'
        return null
      }
      return value
    }
    const required = (key: keyof FormState, message: string): number | null => {
      const value = parseFrNumber(String(form[key]))
      if (value === null || value <= 0) {
        next[key] = message
        return null
      }
      return value
    }

    if (!form.name.trim()) next.name = 'Donnez un nom à cet objectif.'
    const input: GoalInput = {
      name: form.name.trim(),
      icon: form.icon.trim() || null,
      kind: form.kind,
      target_amount: null,
      target_date: null,
      monthly_contribution: null,
      expected_return_pct: null,
      monthly_expenses: null,
      coverage_months: null,
      account_ids: form.account_ids,
      member_ids: form.member_ids,
    }
    if (form.kind === 'project') {
      input.target_amount = required('target_amount', 'Indiquez le montant à atteindre.')
      input.target_date = form.target_date || null
      input.monthly_contribution = optional('monthly_contribution')
      input.expected_return_pct = optional('expected_return_pct', -100)
    } else {
      input.monthly_expenses = required('monthly_expenses', 'Indiquez vos dépenses mensuelles.')
      input.coverage_months = required('coverage_months', 'Indiquez le nombre de mois à couvrir.')
    }
    if (form.account_ids.length === 0) next.account_ids = 'Liez au moins un compte : sa valeur mesure la progression.'
    setErrors(next)
    if (Object.keys(next).length > 0) return
    mutation.mutate(input)
  }

  const eligible = accounts.filter((account) => !account.is_liability)

  return (
    <Modal
      title={goal ? 'Modifier l’objectif' : 'Créer un objectif'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="goal-form" busy={mutation.isPending}>
            {goal ? 'Enregistrer' : 'Créer l’objectif'}
          </Button>
        </>
      }
    >
      <form id="goal-form" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <Segmented
          label="Type d’objectif"
          value={form.kind}
          onChange={(kind) => setForm((f) => ({ ...f, kind }))}
          options={[
            { value: 'project', label: 'Projet' },
            { value: 'safety_net', label: 'Matelas de sécurité' },
          ]}
        />
        <p className="-mt-1 text-[13.5px] text-muted">
          {form.kind === 'project'
            ? 'Un montant à réunir pour une date : apport immobilier, études, retraite…'
            : 'Une réserve disponible pour couvrir plusieurs mois de dépenses en cas d’imprévu.'}
        </p>
        <div className="grid gap-4 sm:grid-cols-[1fr_7rem]">
          <TextField label="Nom" value={form.name} onChange={text('name')} error={errors.name} />
          <TextField label="Pictogramme" value={form.icon} onChange={text('icon')} hint="Un emoji, facultatif." maxLength={4} />
        </div>

        {form.kind === 'project' ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <NumberField
              label="Montant à atteindre"
              value={form.target_amount}
              onChange={text('target_amount')}
              error={errors.target_amount}
              suffix="€"
            />
            <TextField
              label="Échéance"
              type="date"
              value={form.target_date}
              onChange={text('target_date')}
              hint="Facultatif."
            />
            <NumberField
              label="Versement mensuel prévu"
              value={form.monthly_contribution}
              onChange={text('monthly_contribution')}
              error={errors.monthly_contribution}
              suffix="€"
            />
            <NumberField
              label="Rendement annuel attendu"
              value={form.expected_return_pct}
              onChange={text('expected_return_pct')}
              error={errors.expected_return_pct}
              suffix="%"
              hint="Votre hypothèse, par exemple 5."
            />
          </div>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2">
            <NumberField
              label="Dépenses mensuelles"
              value={form.monthly_expenses}
              onChange={text('monthly_expenses')}
              error={errors.monthly_expenses}
              suffix="€"
            />
            <NumberField
              label="Mois à couvrir"
              value={form.coverage_months}
              onChange={text('coverage_months')}
              error={errors.coverage_months}
              suffix="mois"
              hint="Souvent 3 à 6 mois."
            />
          </div>
        )}

        <CheckList
          legend="Comptes affectés à cet objectif"
          items={eligible.map((account) => ({ id: account.id, label: account.name, sub: money(account.value) }))}
          selected={form.account_ids}
          onChange={(ids) => setForm((f) => ({ ...f, account_ids: ids }))}
          empty="Aucun compte. Créez d’abord un compte dans l’écran Comptes."
        />
        {errors.account_ids ? <p className="-mt-2 text-[13px] text-neg">{errors.account_ids}</p> : null}

        <CheckList
          legend="Membres concernés"
          hint="Facultatif."
          items={members.map((member) => ({ id: member.id, label: member.name }))}
          selected={form.member_ids}
          onChange={(ids) => setForm((f) => ({ ...f, member_ids: ids }))}
          empty="Aucun membre dans le foyer."
        />
        <FormError error={mutation.error} />
      </form>
    </Modal>
  )
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-[13px] text-muted">{label}</dt>
      <dd className="num">{children}</dd>
    </div>
  )
}

function GoalCard({
  goal,
  accounts,
  members,
  onEdit,
  onDelete,
}: {
  goal: Goal
  accounts: Account[]
  members: Member[]
  onEdit: () => void
  onDelete: () => void
}) {
  const { progress } = goal
  const status = GOAL_STATUS[progress.status] ?? { label: progress.status, tone: 'accent' as const }
  const linkedAccounts = goal.account_ids.map((id) => accounts.find((account) => account.id === id)?.name).filter(Boolean)
  const linkedMembers = goal.member_ids.map((id) => members.find((member) => member.id === id)?.name).filter(Boolean)
  const required = progress.required_return_pct
  const expected = goal.expected_return_pct

  return (
    <article className="panel flex flex-col gap-4 px-4 py-4 sm:px-5 sm:py-5">
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-[18px] font-semibold">
            {goal.icon ? <span className="mr-2">{goal.icon}</span> : null}
            {goal.name}
          </h2>
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[13.5px] text-muted">
            <Badge tone={status.tone}>{status.label}</Badge>
            <span>{goal.kind === 'project' ? 'Projet' : 'Matelas de sécurité'}</span>
          </div>
        </div>
        <div className="flex shrink-0">
          <IconButton icon="edit" label={`Modifier ${goal.name}`} onClick={onEdit} />
          <IconButton icon="trash" label={`Supprimer ${goal.name}`} onClick={onDelete} />
        </div>
      </header>

      <div>
        <div className="mb-2 flex flex-wrap items-baseline justify-between gap-x-3">
          <span className="num font-display text-[24px] font-semibold">{money(progress.current_amount)}</span>
          <span className="num text-muted">
            sur {money(progress.target_amount)}, soit {pct(progress.pct, 0)}
          </span>
        </div>
        <ProgressBar pct={progress.pct} tone={status.tone} />
      </div>

      <dl className="grid grid-cols-2 gap-x-5 gap-y-3 text-[14px] sm:grid-cols-3">
        {goal.kind === 'project' ? (
          <>
            <Fact label="Échéance">{goal.target_date ? date(goal.target_date, 'long') : 'Sans date'}</Fact>
            <Fact label="Versement mensuel">
              {goal.monthly_contribution !== null ? money(goal.monthly_contribution) : 'Non renseigné'}
            </Fact>
            <Fact label="Montant projeté à l’échéance">
              {progress.projected_amount_at_date !== null ? money(progress.projected_amount_at_date) : '–'}
            </Fact>
            <Fact label="Rendement nécessaire">{required !== null ? `${pct(required, 1)} par an` : '–'}</Fact>
            <Fact label="Rendement attendu">{expected !== null ? `${pct(expected, 1)} par an` : 'Non renseigné'}</Fact>
          </>
        ) : (
          <>
            <Fact label="Couverture actuelle">
              {progress.coverage_months !== null ? `${number(progress.coverage_months, 1)} mois` : '–'}
            </Fact>
            <Fact label="Couverture visée">{goal.coverage_months !== null ? `${number(goal.coverage_months, 1)} mois` : '–'}</Fact>
            <Fact label="Dépenses mensuelles">{money(goal.monthly_expenses)}</Fact>
          </>
        )}
      </dl>

      {goal.kind === 'project' && required !== null && expected !== null ? (
        <p className="text-[14px] text-muted">
          {required <= expected
            ? 'Votre hypothèse de rendement suffit pour atteindre la cible à la date prévue.'
            : `Il manque ${pct(required - expected, 1)} de rendement annuel : augmentez le versement, repoussez l’échéance ou réduisez la cible.`}
        </p>
      ) : null}

      <p className="border-t border-line pt-3 text-[13.5px] text-muted">
        {linkedAccounts.length > 0 ? `Comptes : ${linkedAccounts.join(', ')}.` : 'Aucun compte lié.'}
        {linkedMembers.length > 0 ? ` Membres : ${linkedMembers.join(', ')}.` : ''}
      </p>
    </article>
  )
}

export default function GoalsPage() {
  const goals = useGoals()
  const accounts = useAccounts()
  const members = useMembers()
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Goal | null>(null)
  const [deleting, setDeleting] = useState<Goal | null>(null)
  const remove = useDeleteGoal(() => setDeleting(null))

  const addButton = (
    <Button variant="primary" icon="plus" onClick={() => setCreating(true)}>
      Créer un objectif
    </Button>
  )

  return (
    <>
      <PageHeader
        title="Objectifs"
        description="Affectez des comptes à un projet ou à votre matelas de sécurité et suivez où vous en êtes."
        actions={goals.isSuccess && goals.data.length > 0 ? addButton : undefined}
      />

      {goals.isPending ? (
        <LoadingBlock rows={3} />
      ) : goals.isError ? (
        <ErrorState error={goals.error} onRetry={() => void goals.refetch()} />
      ) : goals.data.length === 0 ? (
        <EmptyState title="Aucun objectif" action={addButton}>
          Commencez par un matelas de sécurité : indiquez vos dépenses mensuelles, le nombre de mois à couvrir et le
          livret qui sert de réserve. Ajoutez ensuite vos projets avec leur montant et leur échéance.
        </EmptyState>
      ) : (
        <div className="grid gap-4 sm:gap-5 xl:grid-cols-2">
          {goals.data.map((goal) => (
            <GoalCard
              key={goal.id}
              goal={goal}
              accounts={accounts.data ?? []}
              members={members.data ?? []}
              onEdit={() => setEditing(goal)}
              onDelete={() => {
                remove.reset()
                setDeleting(goal)
              }}
            />
          ))}
        </div>
      )}

      {creating || editing ? (
        <GoalFormModal
          goal={editing ?? undefined}
          accounts={accounts.data ?? []}
          members={members.data ?? []}
          onClose={() => {
            setCreating(false)
            setEditing(null)
          }}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer cet objectif ?"
          confirmLabel="Supprimer l’objectif"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.name} » sera supprimé. Les comptes liés ne sont pas modifiés.
        </ConfirmModal>
      ) : null}
    </>
  )
}
