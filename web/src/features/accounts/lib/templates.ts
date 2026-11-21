// Modèles d'import prêts à l'emploi pour des formats d'export courants de banques et courtiers français.
// Données déclaratives : en-têtes génériques (aucun établissement nommé), correspondance champ → colonne.
import {
  IMPORT_FIELDS,
  guessHoldingMapping,
  guessOperationMapping,
  guessTransactionMapping,
  mappingFromColumns,
  normalize,
  type ImportKind,
  type Mapping,
} from './csv'

export type ImportTemplate = {
  id: string
  label: string
  kind: ImportKind
  /** En-têtes caractéristiques du format, tels qu'exportés. */
  headers: string[]
  /** Champ → en-tête ; chaque en-tête cité figure dans `headers`. */
  columns: Record<string, string>
}

export const IMPORT_TEMPLATES: ImportTemplate[] = [
  {
    id: 'banque-debit-credit',
    label: 'Banque de réseau — relevé débit / crédit',
    kind: 'transactions',
    headers: ['Date opération', 'Date valeur', 'Libellé', 'Débit', 'Crédit'],
    columns: { date: 'Date opération', label: 'Libellé', debit: 'Débit', credit: 'Crédit' },
  },
  {
    id: 'banque-detaillee',
    label: 'Banque de réseau — export détaillé',
    kind: 'transactions',
    headers: [
      'Date de comptabilisation',
      'Libellé simplifié',
      'Libellé opération',
      'Référence',
      'Type opération',
      'Catégorie',
      'Débit',
      'Crédit',
    ],
    columns: {
      date: 'Date de comptabilisation',
      label: 'Libellé simplifié',
      category: 'Catégorie',
      debit: 'Débit',
      credit: 'Crédit',
    },
  },
  {
    id: 'banque-en-ligne',
    label: 'Banque en ligne — montant signé et catégorie',
    kind: 'transactions',
    headers: ['Date', 'Libellé', 'Montant', 'Catégorie', 'Sous-catégorie', 'Solde'],
    columns: { date: 'Date', label: 'Libellé', amount: 'Montant', category: 'Sous-catégorie' },
  },
  {
    id: 'courtier-portefeuille',
    label: 'Courtier — état du portefeuille',
    kind: 'holdings',
    headers: ['Libellé', 'Code ISIN', 'Quantité', 'PRU', 'Dernier cours', 'Valorisation', '+/- value'],
    columns: { name: 'Libellé', isin: 'Code ISIN', quantity: 'Quantité', buy_price: 'PRU', current_price: 'Dernier cours' },
  },
  {
    id: 'assurance-vie-supports',
    label: 'Assurance-vie — répartition par support',
    kind: 'holdings',
    headers: ['Support', 'Code ISIN', 'Nombre de parts', 'Prix de revient', 'Valeur liquidative', 'Montant'],
    columns: {
      name: 'Support',
      isin: 'Code ISIN',
      quantity: 'Nombre de parts',
      buy_price: 'Prix de revient',
      current_price: 'Valeur liquidative',
    },
  },
  {
    id: 'courtier-historique-ordres',
    label: 'Courtier — historique des ordres',
    kind: 'operations',
    headers: ['Date d’exécution', 'Sens', 'Libellé valeur', 'Code ISIN', 'Quantité', 'Cours', 'Frais', 'Montant net'],
    columns: {
      date: 'Date d’exécution',
      kind: 'Sens',
      name: 'Libellé valeur',
      isin: 'Code ISIN',
      quantity: 'Quantité',
      unit_price: 'Cours',
      fees: 'Frais',
      amount: 'Montant net',
    },
  },
  {
    id: 'courtier-mouvements',
    label: 'Courtier en ligne — mouvements du compte',
    kind: 'operations',
    headers: ['Date', 'Type d’opération', 'Instrument', 'ISIN', 'Qté', 'Prix unitaire', 'Commission', 'Débit', 'Crédit', 'Commentaire'],
    columns: {
      date: 'Date',
      kind: 'Type d’opération',
      name: 'Instrument',
      isin: 'ISIN',
      quantity: 'Qté',
      unit_price: 'Prix unitaire',
      fees: 'Commission',
      debit: 'Débit',
      credit: 'Crédit',
      note: 'Commentaire',
    },
  },
]

/** En dessous de ce score, aucun modèle n'est proposé automatiquement. */
export const DETECTION_THRESHOLD = 0.6

/**
 * Score de correspondance entre un modèle et les en-têtes d'un fichier, entre 0 et 1 (indice de Jaccard sur les
 * en-têtes normalisés). 0 si une colonne utilisée par le modèle manque : il ne pourrait pas lire le fichier.
 */
export function templateScore(template: ImportTemplate, headers: string[]): number {
  const file = new Set(headers.map(normalize).filter(Boolean))
  const expected = new Set(template.headers.map(normalize))
  if (Object.values(template.columns).some((header) => !file.has(normalize(header)))) return 0
  const common = [...expected].filter((header) => file.has(header)).length
  return common / (expected.size + file.size - common)
}

export type DetectedTemplate = { template: ImportTemplate; score: number }

/** Meilleur modèle pour ces en-têtes parmi les types autorisés, s'il atteint le seuil. */
export function detectTemplate(headers: string[], kinds: readonly ImportKind[]): DetectedTemplate | null {
  let best: DetectedTemplate | null = null
  for (const template of IMPORT_TEMPLATES) {
    if (!kinds.includes(template.kind)) continue
    const score = templateScore(template, headers)
    if (score >= DETECTION_THRESHOLD && (!best || score > best.score)) best = { template, score }
  }
  return best
}

export const templatesOf = (kind: ImportKind) => IMPORT_TEMPLATES.filter((template) => template.kind === kind)
export const findTemplate = (id: string | null | undefined) =>
  IMPORT_TEMPLATES.find((template) => template.id === id) ?? null

/** Correspondance devinée à partir des seuls noms de colonnes, faute de mieux. */
export function guessMapping(kind: ImportKind, headers: string[]): Mapping<string> {
  if (kind === 'transactions') return guessTransactionMapping(headers)
  if (kind === 'holdings') return guessHoldingMapping(headers)
  return guessOperationMapping(headers)
}

/** Correspondance par nom de colonne appliquée au fichier, ou `null` si elle ne lui convient pas. */
export const applyColumns = (kind: ImportKind, columns: Record<string, string>, headers: string[]) =>
  mappingFromColumns<string>(IMPORT_FIELDS[kind], columns, headers)

/** Préférence mémorisée d'un compte, réduite à ce qui sert à préparer un import. */
export type SavedColumns = { kind: ImportKind; template: string | null; columns: Record<string, string> }

export type ImportSetup = {
  kind: ImportKind
  mapping: Mapping<string>
  /** D'où vient la correspondance proposée. */
  source: 'saved' | 'template' | 'guess'
  template: ImportTemplate | null
}

/**
 * Prépare l'import d'un fichier sans rien demander quand c'est possible : la préférence mémorisée du compte si
 * elle convient au fichier, sinon le modèle reconnu, sinon une correspondance devinée pour le type courant.
 */
export function resolveImportSetup(
  headers: string[],
  current: ImportKind,
  kinds: readonly ImportKind[],
  saved: SavedColumns[],
): ImportSetup {
  const ordered = [current, ...kinds.filter((kind) => kind !== current)]
  for (const kind of ordered) {
    const profile = saved.find((item) => item.kind === kind)
    const mapping = profile ? applyColumns(kind, profile.columns, headers) : null
    if (profile && mapping) return { kind, mapping, source: 'saved', template: findTemplate(profile.template) }
  }
  const detected = detectTemplate(headers, kinds)
  const mapping = detected ? applyColumns(detected.template.kind, detected.template.columns, headers) : null
  if (detected && mapping) return { kind: detected.template.kind, mapping, source: 'template', template: detected.template }
  return { kind: current, mapping: guessMapping(current, headers), source: 'guess', template: null }
}
