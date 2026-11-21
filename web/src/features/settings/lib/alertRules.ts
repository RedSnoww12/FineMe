import type { AlertRule, AlertRuleInput, AlertRuleKind } from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'

export const ALERT_RULE_TEXT: Record<AlertRuleKind, { label: string; description: string }> = {
  goals: {
    label: 'Objectifs',
    description: 'Prévient quand un objectif est atteint ou quand sa date cible est passée.',
  },
  savings_ceiling: {
    label: 'Plafond des livrets',
    description: 'Prévient quand un compte d’épargne approche de son plafond.',
  },
  net_worth_change: {
    label: 'Variation du patrimoine',
    description: 'Prévient d’une forte hausse ou baisse du patrimoine net entre deux instantanés.',
  },
  connections: {
    label: 'Connexions bancaires',
    description: 'Prévient quand une connexion bancaire a expiré ou est en erreur.',
  },
  stale_prices: {
    label: 'Cours non mis à jour',
    description: 'Prévient quand un cours automatique n’est plus actualisé.',
  },
}

/** Ordre d'affichage dans Paramètres. */
export const ALERT_RULE_ORDER: AlertRuleKind[] = [
  'goals',
  'savings_ceiling',
  'net_worth_change',
  'connections',
  'stale_prices',
]

/** Seuil maximal accepté par l'API, pour les règles qui ont un seuil. */
const MAX_THRESHOLD: Partial<Record<AlertRuleKind, number>> = { savings_ceiling: 100, net_worth_change: 1000 }

export type RuleForm = { threshold: string; ceiling: string }
export type RuleFormErrors = Partial<Record<keyof RuleForm, string>>

export function hasThreshold(rule: AlertRule): boolean {
  return rule.threshold_pct !== null
}

export function hasCeiling(rule: AlertRule): boolean {
  return rule.ceiling !== null
}

/** Vérifie les seuils saisis (format français) ; renvoie la modification à envoyer, ou les erreurs. */
export function readRuleForm(
  rule: AlertRule,
  form: RuleForm,
): { input: AlertRuleInput; errors: null } | { input: null; errors: RuleFormErrors } {
  const errors: RuleFormErrors = {}
  const input: AlertRuleInput = {}
  if (hasThreshold(rule)) {
    const max = MAX_THRESHOLD[rule.kind] ?? 100
    const threshold = parseFrNumber(form.threshold)
    if (threshold === null || threshold <= 0 || threshold > max) {
      errors.threshold = `Indiquez un pourcentage supérieur à 0 et au plus ${max.toLocaleString('fr-FR')}.`
    } else {
      input.threshold_pct = threshold
    }
  }
  if (hasCeiling(rule)) {
    const ceiling = parseFrNumber(form.ceiling)
    if (ceiling === null || ceiling <= 0) errors.ceiling = 'Indiquez un plafond supérieur à 0.'
    else input.ceiling = ceiling
  }
  return Object.keys(errors).length > 0 ? { input: null, errors } : { input, errors: null }
}
