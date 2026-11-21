import { useState } from 'react'
import { Link } from 'react-router'
import { todayIso } from '@/shared/lib/format'
import { useDiscreet } from '@/shared/state/discreet'
import { useView } from '@/shared/state/view'
import { Button, FormError, Notice, Panel, SelectField } from '@/shared/ui/ui'
import { useExportTables, useScopeOptions, type ExportTables } from '../hooks/useExport'
import { downloadFile } from '../lib/download'
import { exportFileName } from '../lib/report'
import { toCsv } from '../lib/tabular'
import { toXlsx } from '../lib/xlsx'

type Format = 'xlsx' | 'accounts' | 'holdings' | 'transactions'

const XLSX_TYPE = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'

function save(tables: ExportTables, format: Format, scope: string) {
  const day = todayIso()
  if (format === 'xlsx') {
    downloadFile(
      toXlsx([tables.accounts, tables.holdings, tables.transactions]),
      exportFileName('patrimoine', scope, day, 'xlsx'),
      XLSX_TYPE,
    )
    return
  }
  const table = tables[format]
  downloadFile(toCsv(table), exportFileName(table.name, scope, day, 'csv'), 'text/csv;charset=utf-8')
}

export function ExportPanel() {
  const { memberId } = useView()
  const { hidden } = useDiscreet()
  const { options, labelOf } = useScopeOptions()
  const [scope, setScope] = useState(memberId ?? '')
  const [format, setFormat] = useState<Format>('xlsx')
  const exportTables = useExportTables()
  const reportQuery = scope ? `?membre=${encodeURIComponent(scope)}` : '?membre=foyer'

  return (
    <Panel title="Rapport et exports">
      <div className="flex flex-col gap-5">
        <SelectField
          label="Périmètre"
          value={scope}
          onChange={setScope}
          options={options}
          hint="Les montants détenus appliquent les parts de détention du membre choisi."
          fieldClassName="max-w-[320px]"
        />

        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="max-w-[56ch] text-muted">
            Rapport daté à imprimer ou enregistrer en PDF : synthèse, répartition, évolution, objectifs et liste des
            comptes, lisible par une banque ou un conseiller.
          </p>
          <Link
            to={`/parametres/rapport${reportQuery}`}
            className="inline-flex min-h-10 items-center rounded-[9px] border border-line px-4 font-medium hover:border-muted"
          >
            Préparer le rapport PDF
          </Link>
        </div>

        <div className="flex flex-wrap items-end justify-between gap-3 border-t border-line pt-5">
          <SelectField
            label="Fichier"
            value={format}
            onChange={(value) => setFormat(value as Format)}
            options={[
              { value: 'xlsx', label: 'Excel (.xlsx) : comptes, positions et transactions' },
              { value: 'accounts', label: 'CSV : comptes' },
              { value: 'holdings', label: 'CSV : positions' },
              { value: 'transactions', label: 'CSV : transactions' },
            ]}
            fieldClassName="min-w-0 flex-1 max-w-[420px]"
          />
          <Button icon="download" busy={exportTables.isPending} onClick={() =>
              exportTables.mutate(scope || null, { onSuccess: (tables) => save(tables, format, labelOf(scope || null)) })
            }>
            Exporter
          </Button>
        </div>
        <p className="text-[13.5px] text-muted">
          CSV au format français : point-virgule, virgule décimale, dates JJ/MM/AAAA, encodage UTF-8 reconnu par Excel.
        </p>
        {hidden ? (
          <Notice>Mode discret actif : les fichiers exportés contiennent tout de même les montants en clair.</Notice>
        ) : null}
        <FormError error={exportTables.error} />
      </div>
    </Panel>
  )
}
