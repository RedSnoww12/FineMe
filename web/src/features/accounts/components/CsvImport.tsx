import { useMemo, useState, type ChangeEvent } from 'react'
import {
  useImportHoldings,
  useImportOperations,
  useImportProfiles,
  useImportTransactions,
  useSaveImportProfile,
} from '../hooks/useAccountsApi'
import {
  buildHoldings,
  buildOperations,
  buildTransactions,
  columnsOf,
  decodeCsvBytes,
  holdingMappingIssues,
  importReport,
  operationMappingIssues,
  parseCsv,
  transactionMappingIssues,
  type HoldingField,
  type ImportKind,
  type ImportReport,
  type Mapping,
  type OperationField,
  type ParsedCsv,
  type TransactionField,
} from '../lib/csv'
import { OPERATION_KIND_LABEL } from '../lib/operations'
import { applyColumns, findTemplate, guessMapping, resolveImportSetup, templatesOf, type ImportSetup } from '../lib/templates'
import { parseFrNumber } from '@/shared/lib/parse'
import { date, money, number } from '@/shared/lib/format'
import { Button, FormError, Modal, Notice, NumberField, Segmented, SelectField } from '@/shared/ui/ui'

const FIELD_LABELS: Record<ImportKind, { key: string; label: string }[]> = {
  transactions: [
    { key: 'date', label: 'Date' },
    { key: 'label', label: 'Libellé' },
    { key: 'amount', label: 'Montant signé' },
    { key: 'debit', label: 'Débit' },
    { key: 'credit', label: 'Crédit' },
    { key: 'category', label: 'Catégorie' },
  ],
  holdings: [
    { key: 'name', label: 'Nom' },
    { key: 'isin', label: 'Code ISIN' },
    { key: 'quantity', label: 'Quantité' },
    { key: 'buy_price', label: 'Prix d’achat unitaire' },
    { key: 'current_price', label: 'Cours actuel' },
  ],
  operations: [
    { key: 'date', label: 'Date' },
    { key: 'kind', label: 'Type d’opération' },
    { key: 'name', label: 'Titre' },
    { key: 'isin', label: 'Code ISIN' },
    { key: 'quantity', label: 'Quantité' },
    { key: 'unit_price', label: 'Cours unitaire' },
    { key: 'fees', label: 'Frais' },
    { key: 'amount', label: 'Montant' },
    { key: 'debit', label: 'Débit' },
    { key: 'credit', label: 'Crédit' },
    { key: 'note', label: 'Note' },
  ],
}

const KIND_HINT: Record<ImportKind, string> = {
  transactions: 'Utilisez soit « Montant signé », soit « Débit » et « Crédit » si votre banque les sépare.',
  holdings: '',
  operations:
    'Le type est lu dans le libellé (achat, vente, dividende, frais, versement, retrait). Sans cours, il se déduit du montant net et des frais.',
}

const DELIMITER_NAME: Record<string, string> = {
  ';': 'point-virgule',
  ',': 'virgule',
  '\t': 'tabulation',
  '|': 'barre verticale',
}

const plural = (count: number, word: string) => `${count} ${word}${count > 1 ? 's' : ''}`

function setupMessage(setup: ImportSetup): string {
  if (setup.source === 'saved') return 'Correspondance mémorisée pour ce compte appliquée : aucun réglage nécessaire.'
  if (setup.source === 'template' && setup.template) return `Format reconnu : ${setup.template.label}.`
  return 'Format non reconnu : vérifiez la correspondance des colonnes.'
}

function ImportReportView({ report }: { report: ImportReport }) {
  return (
    <section aria-labelledby="import-report" className="flex flex-col gap-2 rounded-[11px] border border-line px-4 py-3">
      <h3 id="import-report" className="text-[15px] font-semibold">
        Rapport d’import
      </h3>
      <dl className="grid grid-cols-3 gap-2 text-[14px]">
        <div>
          <dt className="text-muted">Ajoutées</dt>
          <dd className="num font-semibold">{report.added}</dd>
        </div>
        <div>
          <dt className="text-muted">Ignorées (doublons)</dt>
          <dd className="num font-semibold">{report.skipped}</dd>
        </div>
        <div>
          <dt className="text-muted">En erreur</dt>
          <dd className="num font-semibold">{report.errors.length}</dd>
        </div>
      </dl>
      {report.errors.length > 0 ? (
        <ul className="text-[13.5px] text-muted">
          {report.errors.map((error, i) => (
            <li key={`${error.line}-${i}`}>
              Ligne {error.line} : {error.reason}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  )
}

type Props = {
  accountId: string
  currency: string
  /** Positions et journal d'opérations n'ont pas de sens pour un compte courant, un bien ou un prêt. */
  allowHoldings: boolean
  onClose: () => void
}

export function CsvImportModal({ accountId, currency, allowHoldings, onClose }: Props) {
  const kinds: ImportKind[] = allowHoldings ? ['transactions', 'holdings', 'operations'] : ['transactions']
  const profiles = useImportProfiles(accountId)
  const saveProfile = useSaveImportProfile(accountId)
  const [kind, setKind] = useState<ImportKind>('transactions')
  const [fileName, setFileName] = useState('')
  const [csv, setCsv] = useState<ParsedCsv | null>(null)
  const [fileError, setFileError] = useState('')
  const [mappings, setMappings] = useState<Partial<Record<ImportKind, Mapping<string>>>>({})
  const [templates, setTemplates] = useState<Partial<Record<ImportKind, string>>>({})
  const [setupNotice, setSetupNotice] = useState('')
  const [templateError, setTemplateError] = useState('')
  const [newBalance, setNewBalance] = useState('')
  const [replace, setReplace] = useState(false)
  const [done, setDone] = useState('')
  const [report, setReport] = useState<ImportReport | null>(null)

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    setDone('')
    setReport(null)
    setFileError('')
    setTemplateError('')
    if (!file) return
    try {
      const parsed = parseCsv(decodeCsvBytes(await file.arrayBuffer()))
      if (parsed.rows.length === 0) {
        setCsv(null)
        setFileError('Ce fichier ne contient aucune ligne de données sous la ligne d’en-tête.')
        return
      }
      const setup = resolveImportSetup(parsed.headers, kind, kinds, profiles.data ?? [])
      const initial: Partial<Record<ImportKind, Mapping<string>>> = {}
      for (const k of kinds) initial[k] = k === setup.kind ? setup.mapping : guessMapping(k, parsed.headers)
      setFileName(file.name)
      setCsv(parsed)
      setKind(setup.kind)
      setMappings(initial)
      setTemplates(setup.template ? { [setup.kind]: setup.template.id } : {})
      setSetupNotice(setupMessage(setup))
    } catch {
      setCsv(null)
      setFileError('Lecture impossible. Vérifiez qu’il s’agit bien d’un fichier CSV.')
    }
  }

  const mapping = mappings[kind]
  const transactions = useMemo(
    () => (csv && mapping && kind === 'transactions' ? buildTransactions(csv.rows, mapping as Mapping<TransactionField>) : null),
    [csv, mapping, kind],
  )
  const holdings = useMemo(
    () => (csv && mapping && kind === 'holdings' ? buildHoldings(csv.rows, mapping as Mapping<HoldingField>, currency) : null),
    [csv, mapping, kind, currency],
  )
  const operations = useMemo(
    () => (csv && mapping && kind === 'operations' ? buildOperations(csv.rows, mapping as Mapping<OperationField>) : null),
    [csv, mapping, kind],
  )
  const built = transactions ?? holdings ?? operations

  const issues = !mapping
    ? []
    : kind === 'transactions'
      ? transactionMappingIssues(mapping as Mapping<TransactionField>)
      : kind === 'holdings'
        ? holdingMappingIssues(mapping as Mapping<HoldingField>)
        : operationMappingIssues(mapping as Mapping<OperationField>)
  const balanceValue = newBalance.trim() ? parseFrNumber(newBalance) : null
  const balanceError = newBalance.trim() && balanceValue === null ? 'Saisissez un nombre, par exemple 1 234,56.' : undefined

  /** Après un import réussi : mémorise la correspondance pour le prochain fichier du même établissement. */
  const finish = (message: string, result: { inserted: number; skipped: number; errors?: { index: number; reason: string }[] }) => {
    if (csv && mapping && built) {
      setReport(importReport(built, result))
      saveProfile.mutate({ kind, template: templates[kind] ?? null, columns: columnsOf(mapping, csv.headers) })
    }
    setDone(message)
    setCsv(null)
  }

  const importTransactions = useImportTransactions(accountId, (result) =>
    finish(
      `${plural(result.inserted, 'opération')} importée${result.inserted > 1 ? 's' : ''}` +
        (result.skipped > 0
          ? `, ${result.skipped} déjà présente${result.skipped > 1 ? 's' : ''} ignorée${result.skipped > 1 ? 's' : ''}.`
          : '.'),
      result,
    ),
  )
  const importHoldings = useImportHoldings(accountId, (imported) =>
    finish(`${plural(imported, 'position')} importée${imported > 1 ? 's' : ''}.`, { inserted: imported, skipped: 0 }),
  )
  const importOperations = useImportOperations(accountId, (result) =>
    finish(
      `${plural(result.inserted, 'opération')} ajoutée${result.inserted > 1 ? 's' : ''} au journal.`,
      result,
    ),
  )
  const mutation = { transactions: importTransactions, holdings: importHoldings, operations: importOperations }[kind]
  const pending = importTransactions.isPending || importHoldings.isPending || importOperations.isPending
  const count = built?.items.length ?? 0
  const canImport = Boolean(csv) && issues.length === 0 && count > 0 && !balanceError

  const submit = () => {
    if (!canImport) return
    if (transactions) importTransactions.mutate({ transactions: transactions.items, newCashBalance: balanceValue })
    if (holdings) importHoldings.mutate({ holdings: holdings.items, replace })
    if (operations) importOperations.mutate(operations.items)
  }

  const chooseTemplate = (id: string) => {
    setTemplateError('')
    const template = findTemplate(id)
    setTemplates({ ...templates, [kind]: template?.id })
    if (!template || !csv) return
    const applied = applyColumns(kind, template.columns, csv.headers)
    if (applied) setMappings({ ...mappings, [kind]: applied })
    else setTemplateError('Ce modèle ne correspond pas aux colonnes de ce fichier.')
  }

  const columnOptions = (csv?.headers ?? []).map((header, index) => {
    const sample = csv?.rows[0]?.[index]?.trim()
    return { value: String(index), label: sample ? `${header} (ex. ${sample.slice(0, 24)})` : header }
  })
  const options = [{ value: '-1', label: 'Ne pas utiliser' }, ...columnOptions]
  const templateOptions = [
    { value: '', label: 'Aucun (correspondance libre)' },
    ...templatesOf(kind).map((template) => ({ value: template.id, label: template.label })),
  ]
  const importLabel = {
    transactions: `Importer ${plural(count, 'opération')}`,
    holdings: `Importer ${plural(count, 'position')}`,
    operations: `Importer ${plural(count, 'ligne')} du journal`,
  }[kind]

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
              {importLabel}
            </Button>
          ) : null}
        </>
      }
    >
      <div className="flex flex-col gap-5">
        {done ? <Notice tone="pos">{done}</Notice> : null}
        {report ? <ImportReportView report={report} /> : null}

        {kinds.length > 1 ? (
          <Segmented
            label="Contenu du fichier"
            value={kind}
            onChange={(value) => {
              setKind(value)
              setTemplateError('')
            }}
            options={[
              { value: 'transactions', label: 'Opérations' },
              { value: 'holdings', label: 'Positions' },
              { value: 'operations', label: 'Journal' },
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
            disabled={profiles.isPending}
            onChange={(event) => void onFile(event)}
            className="block w-full text-[14px] file:mr-3 file:min-h-10 file:rounded-[9px] file:border file:border-solid file:border-line file:bg-surface file:px-4 file:font-medium file:text-ink"
          />
          <p className="text-[13px] text-muted">
            La première ligne doit contenir les noms des colonnes. Les nombres « 1 234,56 » et les dates « 31/12/2024 »
            sont reconnus. Le fichier est lu dans votre navigateur ; la correspondance des colonnes est mémorisée pour
            ce compte.
          </p>
          {fileError ? <FormError error={fileError} /> : null}
        </div>

        {csv && mapping ? (
          <>
            <p className="text-[14px] text-muted">
              <span className="font-medium text-ink">{fileName}</span> : {csv.rows.length} ligne
              {csv.rows.length > 1 ? 's' : ''}, {csv.headers.length} colonnes, séparateur{' '}
              {DELIMITER_NAME[csv.delimiter] ?? `« ${csv.delimiter} »`}.
            </p>
            {setupNotice ? <Notice>{setupNotice}</Notice> : null}

            <SelectField
              label="Modèle de fichier"
              value={templates[kind] ?? ''}
              onChange={chooseTemplate}
              options={templateOptions}
              error={templateError || undefined}
              fieldClassName="sm:max-w-md"
            />

            <fieldset className="grid gap-4 sm:grid-cols-2">
              <legend className="mb-3 text-[15px] font-semibold">Correspondance des colonnes</legend>
              {FIELD_LABELS[kind].map((field) => (
                <SelectField
                  key={field.key}
                  label={field.label}
                  value={String(mapping[field.key] ?? -1)}
                  onChange={(value) => setMappings({ ...mappings, [kind]: { ...mapping, [field.key]: Number(value) } })}
                  options={options}
                />
              ))}
            </fieldset>
            {KIND_HINT[kind] ? <p className="-mt-2 text-[13px] text-muted">{KIND_HINT[kind]}</p> : null}

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
                    {transactions ? (
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
                    {holdings ? (
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
                    {operations ? (
                      <table className="table">
                        <thead>
                          <tr>
                            <th>Date</th>
                            <th>Type</th>
                            <th>Titre</th>
                            <th className="r">Quantité</th>
                            <th className="r">Cours</th>
                            <th className="r">Frais</th>
                            <th className="r">Montant</th>
                          </tr>
                        </thead>
                        <tbody>
                          {operations.items.slice(0, 8).map((item, i) => (
                            <tr key={i}>
                              <td className="num whitespace-nowrap">{date(item.date)}</td>
                              <td>{OPERATION_KIND_LABEL[item.kind]}</td>
                              <td>{item.security?.name ?? '–'}</td>
                              <td className="r">{number(item.quantity, 6)}</td>
                              <td className="r">{money(item.unit_price, { decimals: 2, currency })}</td>
                              <td className="r">{money(item.fees, { decimals: 2, currency })}</td>
                              <td className="r">{money(item.amount, { decimals: 2, currency })}</td>
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
            ) : null}
            {kind === 'holdings' ? (
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
            ) : null}

            <FormError error={mutation.error} />
          </>
        ) : null}
      </div>
    </Modal>
  )
}
