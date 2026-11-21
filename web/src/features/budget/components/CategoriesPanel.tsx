import { useState, type FormEvent } from 'react'
import type { BudgetCategory } from '@/shared/api/types'
import { money, toInput } from '@/shared/lib/format'
import { Button, FormError, IconButton, Modal, NumberField, Panel, ProgressBar, TextField } from '@/shared/ui/ui'
import { useSetBudgetTarget } from '../hooks/useBudgetApi'
import { categoryLabel, cleanCategory, parseTarget, targetStatus } from '../lib/budget'

type Editing = { category: string | null; target: number | null }

function TargetModal({ editing, onClose }: { editing: Editing; onClose: () => void }) {
  const [category, setCategory] = useState(editing.category ?? '')
  const [amount, setAmount] = useState(toInput(editing.target, 2))
  const [errors, setErrors] = useState<{ category?: string; amount?: string }>({})
  const save = useSetBudgetTarget(onClose)
  const isNew = editing.category === null

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const name = cleanCategory(category)
    const parsed = parseTarget(amount)
    const next = {
      category: name ? undefined : 'Indiquez une catégorie.',
      amount: 'error' in parsed ? parsed.error : isNew && parsed.value === null ? 'Indiquez un montant.' : undefined,
    }
    setErrors(next)
    if (name && 'value' in parsed && !next.amount) save.mutate({ category: name, monthly_amount: parsed.value })
  }

  return (
    <Modal
      title={isNew ? 'Nouvelle cible mensuelle' : `Cible « ${editing.category} »`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="budget-target" busy={save.isPending}>
            Enregistrer
          </Button>
        </>
      }
    >
      <form id="budget-target" onSubmit={submit} noValidate className="flex flex-col gap-4">
        {isNew ? (
          <TextField label="Catégorie" value={category} onChange={setCategory} error={errors.category} list="budget-categories" />
        ) : null}
        <NumberField
          label="Montant mensuel visé"
          value={amount}
          onChange={setAmount}
          error={errors.amount}
          suffix="€"
          hint={isNew ? undefined : 'Laissez vide pour supprimer la cible.'}
        />
        <FormError error={save.error} />
      </form>
    </Modal>
  )
}

/** Dépenses du mois par catégorie, avec le suivi des cibles mensuelles. */
export function CategoriesPanel({ categories, expenses }: { categories: BudgetCategory[]; expenses: number }) {
  const [editing, setEditing] = useState<Editing | null>(null)
  return (
    <Panel
      title="Dépenses par catégorie"
      action={
        <Button size="sm" icon="plus" onClick={() => setEditing({ category: null, target: null })}>
          Ajouter une cible
        </Button>
      }
    >
      {categories.length === 0 ? (
        <p className="text-muted">Aucune dépense ce mois-ci.</p>
      ) : (
        <ul className="flex flex-col gap-4">
          {categories.map((item) => {
            const status = targetStatus(item)
            const share = expenses > 0 ? (item.amount / expenses) * 100 : 0
            const name = categoryLabel(item.category)
            return (
              <li key={name} className="flex flex-col gap-1.5">
                <div className="flex items-center justify-between gap-3">
                  <span className="min-w-0 truncate font-medium">{name}</span>
                  <span className="flex items-center gap-1">
                    <span className="num">
                      {money(item.amount)}
                      {item.target !== null ? <span className="text-muted"> / {money(item.target)}</span> : null}
                    </span>
                    {item.category !== null ? (
                      <IconButton
                        icon="edit"
                        label={`Modifier la cible de ${name}`}
                        onClick={() => setEditing({ category: item.category, target: item.target })}
                      />
                    ) : null}
                  </span>
                </div>
                <ProgressBar pct={status ? status.pct : share} tone={status?.over ? 'neg' : status ? 'pos' : 'accent'} />
                {status ? (
                  <p className={status.over ? 'text-[13px] text-neg' : 'text-[13px] text-muted'}>
                    {status.over ? `Cible dépassée de ${money(-status.remaining)}` : `Reste ${money(status.remaining)} sur la cible`}
                  </p>
                ) : null}
              </li>
            )
          })}
        </ul>
      )}
      {editing ? <TargetModal editing={editing} onClose={() => setEditing(null)} /> : null}
    </Panel>
  )
}
