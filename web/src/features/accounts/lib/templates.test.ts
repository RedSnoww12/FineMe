import { describe, expect, it } from 'vitest'
import {
  DETECTION_THRESHOLD,
  IMPORT_TEMPLATES,
  detectTemplate,
  findTemplate,
  resolveImportSetup,
  templateScore,
} from '@/features/accounts/lib/templates'
import { IMPORT_FIELDS } from '@/features/accounts/lib/csv'

const ALL = ['transactions', 'holdings', 'operations'] as const
const ORDERS = ['Date d’exécution', 'Sens', 'Libellé valeur', 'Code ISIN', 'Quantité', 'Cours', 'Frais', 'Montant net']

describe('modèles déclarés', () => {
  it('chaque modèle a un identifiant unique, des champs connus et des colonnes présentes dans ses en-têtes', () => {
    expect(new Set(IMPORT_TEMPLATES.map((t) => t.id)).size).toBe(IMPORT_TEMPLATES.length)
    for (const template of IMPORT_TEMPLATES) {
      for (const [field, header] of Object.entries(template.columns)) {
        expect(IMPORT_FIELDS[template.kind] as readonly string[]).toContain(field)
        expect(template.headers).toContain(header)
      }
      // Un fichier qui a exactement les en-têtes du modèle le reconnaît parfaitement.
      expect(templateScore(template, template.headers)).toBe(1)
    }
  })
})

describe('templateScore', () => {
  const orders = findTemplate('courtier-historique-ordres')!

  it('ignore casse, accents et ponctuation des en-têtes', () => {
    const shouted = ORDERS.map((h) => h.toUpperCase().normalize('NFD').replace(/[̀-ͯ]/g, ''))
    expect(templateScore(orders, shouted)).toBe(1)
  })

  it('baisse avec les colonnes en plus ou en moins, sans tomber à zéro', () => {
    const extra = templateScore(orders, [...ORDERS, 'Devise', 'Place'])
    expect(extra).toBeCloseTo(8 / 10)
    const fewer = templateScore(orders, [...ORDERS.slice(0, 7), 'Montant net'])
    expect(fewer).toBe(1)
  })

  it('vaut zéro si une colonne utilisée par le modèle manque', () => {
    expect(templateScore(orders, ORDERS.filter((h) => h !== 'Sens'))).toBe(0)
  })
})

describe('detectTemplate', () => {
  it('reconnaît le meilleur modèle parmi les types autorisés', () => {
    const detected = detectTemplate([...ORDERS, 'Devise'], ALL)
    expect(detected?.template.id).toBe('courtier-historique-ordres')
    expect(detected?.score).toBeGreaterThanOrEqual(DETECTION_THRESHOLD)
    expect(detectTemplate(['Date opération', 'Date valeur', 'Libellé', 'Débit', 'Crédit'], ALL)?.template.id).toBe(
      'banque-debit-credit',
    )
  })

  it('ne propose rien sous le seuil ou pour un type non autorisé', () => {
    expect(detectTemplate(['Date', 'Libellé', 'Montant'], ALL)).toBeNull()
    expect(detectTemplate(ORDERS, ['transactions'])).toBeNull()
    // Toutes les colonnes utiles sont là mais noyées dans un fichier très large : score trop faible.
    const noisy = [...ORDERS, ...Array.from({ length: 10 }, (_, i) => `Autre ${i}`)]
    expect(detectTemplate(noisy, ALL)).toBeNull()
  })
})

describe('resolveImportSetup', () => {
  const headers = ['Jour', 'Description', 'Sortie', 'Entrée']
  const saved = {
    kind: 'transactions' as const,
    template: null,
    columns: { date: 'Jour', label: 'Description', debit: 'Sortie', credit: 'Entrée' },
  }

  it('applique la préférence mémorisée qui convient au fichier, même si les colonnes ont changé d’ordre', () => {
    const setup = resolveImportSetup(['Entrée', 'jour', 'Sortie', 'Description'], 'transactions', ALL, [saved])
    expect(setup.source).toBe('saved')
    expect(setup.mapping).toMatchObject({ date: 1, label: 3, debit: 2, credit: 0, amount: -1, category: -1 })
  })

  it('choisit le type de la préférence ou du modèle reconnu', () => {
    const ops = { kind: 'operations' as const, template: 'courtier-historique-ordres', columns: { date: 'Jour', kind: 'Sortie' } }
    const fromSaved = resolveImportSetup(headers, 'holdings', ALL, [ops])
    expect(fromSaved).toMatchObject({ kind: 'operations', source: 'saved', template: { id: 'courtier-historique-ordres' } })
    const fromTemplate = resolveImportSetup(ORDERS, 'transactions', ALL, [])
    expect(fromTemplate).toMatchObject({ kind: 'operations', source: 'template' })
    expect(fromTemplate.mapping).toMatchObject({ date: 0, kind: 1, name: 2, unit_price: 5, amount: 7, note: -1 })
  })

  it('ignore une préférence dont une colonne manque et devine à défaut de modèle', () => {
    const setup = resolveImportSetup(['Jour', 'Description', 'Montant'], 'transactions', ALL, [saved])
    expect(setup).toMatchObject({ kind: 'transactions', source: 'guess', template: null })
    expect(setup.mapping).toMatchObject({ label: 1, amount: 2 })
  })
})
