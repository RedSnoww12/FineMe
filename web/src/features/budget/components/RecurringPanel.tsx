import type { BudgetRule, RecurringCharge } from '@/shared/api/types'
import { date, money } from '@/shared/lib/format'
import { IconButton, Panel } from '@/shared/ui/ui'
import { useDeleteBudgetRule } from '../hooks/useBudgetApi'
import { categoryLabel } from '../lib/budget'

export function RecurringPanel({ charges }: { charges: RecurringCharge[] }) {
  const total = charges.reduce((sum, charge) => sum + charge.average_amount, 0)
  return (
    <Panel title="Charges récurrentes" action={charges.length > 0 ? <span className="num text-muted">{money(total)} / mois</span> : null}>
      {charges.length === 0 ? (
        <p className="text-muted">
          Aucune charge mensuelle détectée : il faut au moins trois mois de prélèvements au libellé et au montant proches.
        </p>
      ) : (
        <ul className="flex flex-col divide-y divide-line">
          {charges.map((charge) => (
            <li key={`${charge.label}-${charge.last_date}`} className="flex items-center justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
              <div className="min-w-0">
                <div className="truncate font-medium">{charge.label}</div>
                <div className="text-[13px] text-muted">
                  {categoryLabel(charge.category)} · {charge.occurrences} mois · prochaine vers le {date(charge.next_date)}
                </div>
              </div>
              <span className="num shrink-0 font-medium">{money(charge.average_amount, { decimals: 2 })}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}

export function RulesPanel({ rules }: { rules: BudgetRule[] }) {
  const remove = useDeleteBudgetRule()
  if (rules.length === 0) return null
  return (
    <Panel title="Règles de catégorisation">
      <p className="mb-3 text-[14px] text-muted">
        Créées quand vous recatégorisez une transaction : tout libellé qui contient le motif reçoit la catégorie.
      </p>
      <ul className="flex flex-col divide-y divide-line">
        {rules.map((rule) => (
          <li key={rule.id} className="flex items-center justify-between gap-3 py-2">
            <span className="min-w-0">
              <code className="text-[13.5px]">{rule.pattern}</code>
              <span className="text-muted"> → </span>
              <span className="font-medium">{rule.category}</span>
            </span>
            <IconButton
              icon="trash"
              label={`Supprimer la règle « ${rule.pattern} »`}
              disabled={remove.isPending}
              onClick={() => remove.mutate(rule.id)}
            />
          </li>
        ))}
      </ul>
    </Panel>
  )
}
