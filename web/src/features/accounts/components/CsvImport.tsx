import { useMemo, useState, type ChangeEvent } from 'react'
import { useImportHoldings, useImportTransactions } from '../hooks/useAccountsApi'
import {
  buildHoldings,
  buildTransactions,
  decodeCsvBytes,
  guessHoldingMapping,
  guessTransactionMapping,
  holdingMappingIssues,
  parseCsv,
  transactionMappingIssues,
  type HoldingField,
  type ImportKind,
  type Mapping,
  type ParsedCsv,
  type TransactionField,
} from '../lib/csv'
import { parseFrNumber } from '@/shared/lib/parse'
import { date, money, number } from '@/shared/lib/format'
import { Button, FormError, Modal, Notice, NumberField, Segmented, SelectField } from '@/shared/ui/ui'

const TRANSACTION_FIELDS: { key: TransactionField; label: string }[] = [
  { key: 'date', label: 'Date' },
  { key: 'label', label: 'Libellé' },
  { key: 'amount', label: 'Montant signé' },
  { key: 'debit', label: 'Débit' },
  { key: 'credit', label: 'Crédit' },
  { key: 'category', label: 'Catégorie' },
]

const HOLDING_FIELDS: { key: HoldingField; label: string }[] = [
  { key: 'name', label: 'Nom' },
  { key: 'isin', label: 'Code ISIN' },
  { key: 'quantity', label: 'Quantité' },
  { key: 'buy_price', label: 'Prix d’achat unitaire' },
  { key: 'current_price', label: 'Cours actuel' },
]

const DELIMITER_NAME: Record<string, string> = {
  ';': 'point-virgule',
  ',': 'virgule',
  '\t': 'tabulation',
  '|': 'barre verticale',
}

type Props = {
  accountId: string
  currency: string
  /** Les positions n'ont pas de sens pour un compte courant, un bien ou un prêt. */
  allowHoldings: boolean
  onClose: () => void
}

export function CsvImportModal({ accountId, currency, allowHoldings, onClose }: Props) {
  const [kind, setKind] = useState<ImportKind>('transactions')
  const [fileName, setFileName] = useState('')
  const [csv, setCsv] = useState<ParsedCsv | null>(null)
  const [fileError, setFileError] = useState('')
  const [txMapping, setTxMapping] = useState<Mapping<TransactionField> | null>(null)
  const [holdingMapping, setHoldingMapping] = useState<Mapping<HoldingField> | null>(null)
  const [newBalance, setNewBalance] = useState('')
  const [replace, setReplace] = useState(false)
  const [done, setDone] = useState('')

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    setDone('')
    setFileError('')
    if (!file) return
    try {
      const parsed = parseCsv(decodeCsvBytes(await file.arrayBuffer()))
      if (parsed.rows.length === 0) {
        setCsv(null)
        setFileError('Ce fichier ne contient aucune ligne de données sous la ligne d’en-tête.')
        return
      }
      setFileName(file.name)
      setCsv(parsed)
      setTxMapping(guessTransactionMapping(parsed.headers))
      setHoldingMapping(guessHoldingMapping(parsed.headers))
    } catch {
      setCsv(null)
      setFileError('Lecture impossible. Vérifiez qu’il s’agit bien d’un fichier CSV.')
    }
  }

  const transactions = useMemo(
    () => (csv && txMapping && kind === 'transactions' ? buildTransactions(csv.rows, txMapping) : null),
    [csv, txMapping, kind],
  )
  const holdings = useMemo(
    () => (csv && holdingMapping && kind === 'holdings' ? buildHoldings(csv.rows, holdingMapping, currency) : null),
    [csv, holdingMapping, kind, currency],
  )

  const issues =
    kind === 'transactions'
      ? txMapping
        ? transactionMappingIssues(txMapping)
        : []
      : holdingMapping
        ? holdingMappingIssues(holdingMapping)
        : []
  const built = kind === 'transactions' ? transactions : holdings
  const balanceValue = newBalance.trim() ? parseFrNumber(newBalance) : null
  const balanceError = newBalance.trim() && balanceValue === null ? 'Saisissez un nombre, par exemple 1 234,56.' : undefined

  const importTransactions = useImportTransactions(accountId, (result) => {
      setDone(
        `${result.inserted} opération${result.inserted > 1 ? 's' : ''} importée${result.inserted > 1 ? 's' : ''}` +
          (result.skipped > 0 ? `, ${result.skipped} déjà présente${result.skipped > 1 ? 's' : ''} ignorée${result.skipped > 1 ? 's' : ''}.` : '.'),
      )
      setCsv(null)
  })
  const importHoldings = useImportHoldings(accountId, (imported) => {
    setDone(`${imported} position${imported > 1 ? 's' : ''} importée${imported > 1 ? 's' : ''}.`)
    setCsv(null)
  })
  const pending = importTransactions.isPending || importHoldings.isPending
  const mutationError = kind === 'transactions' ? importTransactions.error : importHoldings.error
  const count = built?.items.length ?? 0
  const canImport = Boolean(csv) && issues.length === 0 && count > 0 && !balanceError

  const submit = () => {
    if (!canImport) return
    if (kind === 'transactions' && transactions) importTransactions.mutate({ transactions: transactions.items, newCashBalance: balanceValue })
    if (kind === 'holdings' && holdings) importHoldings.mutate({ holdings: holdings.items, replace })
  }

  const columnOptions = (csv?.headers ?? []).map((header, index) => {
    const sample = csv?.rows[0]?.[index]?.trim()
    return { value: String(index), label: sample ? `${header} (ex. ${sample.slice(0, 24)})` : header }
  })
  const options = [{ value: '-1', label: 'Ne pas utiliser' }, ...columnOptions]

  return (
    <Modal
      title="Importer un fichier CSV"
      wide
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{done ? 'Fermer' : 'Annuler'}</Button>
          {csv ? (
            <Button variant="primary" icon="upload" disabled={!canImport} busy={pending} onClick={submit}>
              {kind === 'transactions'
                ? `Importer ${count} opération${count > 1 ? 's' : ''}`
                : `Importer ${count} position${count > 1 ? 's' : ''}`}
            </Button>
          ) : null}
        </>
      }
    >
      <div className="flex flex-col gap-5">
        {done ? <Notice tone="pos">{done}</Notice> : null}

        {allowHoldings ? (
          <Segmented
            label="Contenu du fichier"
            value={kind}
            onChange={setKind}
            options={[
              { value: 'transactions', label: 'Opérations' },
              { value: 'holdings', label: 'Positions' },
            ]}
          />
        ) : null}

        <div className="flex flex-col gap-1.5">
          <label htmlFor="csv-file" className="text-[13.5px] font-medium">
            Fichier exporté depuis votre banque ou votre courtier
          </label>
          <input
            id="csv-file"
            type="file"
            accept=".csv,.txt,.tsv,text/csv,text/plain"
            onChange={(event) => void onFile(event)}
            className="block w-full text-[14px] file:mr-3 file:min-h-10 file:rounded-[9px] file:border file:border-solid file:border-line file:bg-surface file:px-4 file:font-medium file:text-ink"
          />
          <p className="text-[13px] text-muted">
            La première ligne doit contenir les noms des colonnes. Les nombres « 1 234,56 » et les dates « 31/12/2024 »
            sont reconnus. Le fichier est lu dans votre navigateur.
          </p>
          {fileError ? <FormError error={fileError} /> : null}
        </div>

        {csv ? (
          <>
            <p className="text-[14px] text-muted">
              <span className="font-medium text-ink">{fileName}</span> : {csv.rows.length} ligne
              {csv.rows.length > 1 ? 's' : ''}, {csv.headers.length} colonnes, séparateur{' '}
              {DELIMITER_NAME[csv.delimiter] ?? `« ${csv.delimiter} »`}.
            </p>

            <fieldset className="grid gap-4 sm:grid-cols-2">
              <legend className="mb-3 text-[15px] font-semibold">Correspondance des colonnes</legend>
              {kind === 'transactions' && txMapping
                ? TRANSACTION_FIELDS.map((field) => (
                    <SelectField
                      key={field.key}
                      label={field.label}
                      value={String(txMapping[field.key])}
                      onChange={(value) => setTxMapping({ ...txMapping, [field.key]: Number(value) })}
                      options={options}
                    />
                  ))
                : null}
              {kind === 'holdings' && holdingMapping
                ? HOLDING_FIELDS.map((field) => (
                    <SelectField
                      key={field.key}
                      label={field.label}
                      value={String(holdingMapping[field.key])}
                      onChange={(value) => setHoldingMapping({ ...holdingMapping, [field.key]: Number(value) })}
                      options={options}
                    />
                  ))
                : null}
            </fieldset>
            {kind === 'transactions' ? (
              <p className="-mt-2 text-[13px] text-muted">
                Utilisez soit « Montant signé », soit « Débit » et « Crédit » si votre banque les sépare.
              </p>
            ) : null}

            {issues.length > 0 ? (
              <FormError error={issues.join(' ')} />
            ) : (
              <section className="flex flex-col gap-2.5">
                <h3 className="text-[15px] font-semibold">
                  Aperçu ({Math.min(count, 8)} sur {count})
                </h3>
                {count === 0 ? (
                  <FormError error="Aucune ligne lisible avec cette correspondance. Vérifiez les colonnes choisies." />
                ) : (
                  <div className="table-wrap rounded-[11px] border border-line">
                    {kind === 'transactions' && transactions ? (
                      <table className="table">
                        <thead>
                          <tr>
                            <th>Date</th>
                            <th>Libellé</th>
                            <th>Catégorie</th>
                            <th className="r">Montant</th>
                          </tr>
                        </thead>
                        <tbody>
                          {transactions.items.slice(0, 8).map((item, i) => (
                            <tr key={i}>
                              <td className="num whitespace-nowrap">{date(item.date)}</td>
                              <td>{item.label}</td>
                              <td className="text-muted">{item.category ?? '–'}</td>
                              <td className="r">{money(item.amount, { decimals: 2, currency, signed: true })}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    ) : null}
                    {kind === 'holdings' && holdings ? (
                      <table className="table">
                        <thead>
                          <tr>
                            <th>Nom</th>
                            <th>ISIN</th>
                            <th className="r">Quantité</th>
                            <th className="r">Prix d’achat</th>
                            <th className="r">Cours</th>
                          </tr>
                        </thead>
                        <tbody>
                          {holdings.items.slice(0, 8).map((item, i) => (
                            <tr key={i}>
                              <td>{item.name}</td>
                              <td className="num text-muted">{item.isin ?? '–'}</td>
                              <td className="r">{number(item.quantity, 6)}</td>
                              <td className="r">{money(item.buy_price, { decimals: 2, currency })}</td>
                              <td className="r">{money(item.current_price, { decimals: 2, currency })}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    ) : null}
                  </div>
                )}
                {built && built.errors.length > 0 ? (
                  <div className="rounded-[9px] bg-surface-2 px-3 py-2.5 text-[13.5px]">
                    <p className="font-medium">
                      {built.errors.length} ligne{built.errors.length > 1 ? 's' : ''} ignorée
                      {built.errors.length > 1 ? 's' : ''} car illisible{built.errors.length > 1 ? 's' : ''} :
                    </p>
                    <ul className="mt-1 text-muted">
                      {built.errors.slice(0, 5).map((error) => (
                        <li key={error.line}>
                          Ligne {error.line} : {error.reason}
                        </li>
                      ))}
                      {built.errors.length > 5 ? <li>… et {built.errors.length - 5} autres.</li> : null}
                    </ul>
                  </div>
                ) : null}
              </section>
            )}

            {kind === 'transactions' ? (
              <NumberField
                label="Nouveau solde du compte après import"
                value={newBalance}
                onChange={setNewBalance}
                error={balanceError}
                hint="Facultatif. Laissez vide pour ne pas modifier le solde espèces."
                fieldClassName="sm:max-w-xs"
              />
            ) : (
              <label className="flex items-start gap-2.5">
                <input
                  type="checkbox"
                  checked={replace}
                  onChange={(event) => setReplace(event.target.checked)}
                  className="mt-1 size-4 accent-[var(--accent)]"
                />
                <span>
                  Remplacer les positions existantes de ce compte
                  <span className="block text-[13px] text-muted">
                    Décoché, les positions du fichier s’ajoutent à celles déjà présentes.
                  </span>
                </span>
              </label>
            )}

            <FormError error={mutationError} />
          </>
        ) : null}
      </div>
    </Modal>
  )
}
