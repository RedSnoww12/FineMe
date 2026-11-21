import { useState, type FormEvent } from 'react'
import type { BudgetTransaction } from '@/shared/api/types'
import { date, money } from '@/shared/lib/format'
import { Badge, Button, FormError, IconButton, Modal, Panel, SelectField, Signed, TextField } from '@/shared/ui/ui'
import { useUpdateTransaction } from '../hooks/useBudgetApi'
import { categoryLabel, sourceLabel, transactionPatch, transferChoice, type TransferChoice } from '../lib/budget'

const TRANSFER_OPTIONS: { value: TransferChoice; label: string }[] = [
  { value: 'auto', label: 'Détection automatique' },
  { value: 'yes', label: 'Toujours un virement interne' },
  { value: 'no', label: 'Jamais un virement interne' },
]

function EditModal({ item, onClose }: { item: BudgetTransaction; onClose: () => void }) {
  const [category, setCategory] = useState(item.category ?? '')
  const [transfer, setTransfer] = useState<TransferChoice>(transferChoice(item))
  const update = useUpdateTransaction(onClose)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const patch = transactionPatch(item, { category, transfer })
    if (Object.keys(patch).length === 0) onClose()
    else update.mutate({ id: item.id, patch })
  }

  return (
    <Modal
      title="Catégoriser la transaction"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="budget-transaction" busy={update.isPending}>
            Enregistrer
          </Button>
        </>
      }
    >
      <form id="budget-transaction" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <p className="num">
          {date(item.date)} · {item.label} · <span className="font-semibold">{money(item.amount_eur, { decimals: 2 })}</span>
        </p>
        <TextField
          label="Catégorie"
          value={category}
          onChange={setCategory}
          list="budget-categories"
          hint="Les transactions de même libellé prendront aussi cette catégorie. Videz le champ pour revenir à l’automatique."
        />
        <SelectField
          label="Virement interne"
          value={transfer}
          onChange={(value) => setTransfer(value as TransferChoice)}
          options={TRANSFER_OPTIONS}
          hint="Un virement interne entre comptes du foyer n’est compté ni en revenu ni en dépense."
        />
        <FormError error={update.error} />
      </form>
    </Modal>
  )
}

export function TransactionsPanel({ items }: { items: BudgetTransaction[] }) {
  const [editing, setEditing] = useState<BudgetTransaction | null>(null)
  return (
    <Panel title="Transactions du mois" flush>
      {items.length === 0 ? (
        <p className="px-4 pb-5 text-muted sm:px-5">Aucune transaction ce mois-ci.</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Date</th>
                <th>Libellé</th>
                <th>Catégorie</th>
                <th className="r">Montant</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.id} className={item.internal_transfer ? 'text-muted' : undefined}>
                  <td className="num whitespace-nowrap">{date(item.date)}</td>
                  <td>
                    <div className="font-medium">{item.label}</div>
                    <div className="text-[13px] text-muted">{item.account_name}</div>
                  </td>
                  <td>
                    <div className="flex flex-wrap items-center gap-1.5">
                      {item.internal_transfer ? <Badge tone="brass">Virement interne</Badge> : <span>{categoryLabel(item.category)}</span>}
                      {!item.internal_transfer ? <span className="text-[12.5px] text-muted">{sourceLabel(item.category_source)}</span> : null}
                    </div>
                  </td>
                  <td className="r num whitespace-nowrap">
                    <Signed value={item.amount_eur}>{money(item.amount_eur, { decimals: 2, signed: true })}</Signed>
                  </td>
                  <td>
                    <div className="flex justify-end">
                      <IconButton icon="edit" label={`Catégoriser « ${item.label} »`} onClick={() => setEditing(item)} />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing ? <EditModal item={editing} onClose={() => setEditing(null)} /> : null}
    </Panel>
  )
}
