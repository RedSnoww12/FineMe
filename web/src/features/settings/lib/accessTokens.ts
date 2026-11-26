// Accès IA : présentation des jetons d'accès et des appels MCP, commande de configuration du client.
import type { AccessToken, McpCall } from '@/shared/api/types'

/** Nombre maximal de jetons actifs (règle de l'API). */
export const MAX_ACTIVE_TOKENS = 10

/** Adresse du serveur MCP : celle de l'API si elle est sur un autre domaine, sinon l'origine de la page. */
export function mcpEndpoint(apiBase: string, pageOrigin: string): string {
  return `${(apiBase || pageOrigin).replace(/\/+$/, '')}/mcp`
}

/** Commande Claude Code qui branche FineMe avec ce jeton. */
export function claudeAddCommand(endpoint: string, secret: string): string {
  return `claude mcp add --transport http fineme ${endpoint} --header "Authorization: Bearer ${secret}"`
}

export function isActive(token: AccessToken): boolean {
  return token.revoked_at === null
}

export function activeCount(tokens: readonly AccessToken[]): number {
  return tokens.filter(isActive).length
}

const TOOL_LABELS: Record<string, string> = {
  get_overview: 'Vue d’ensemble',
  get_performance: 'Performance',
  get_budget: 'Budget',
  get_goals: 'Objectifs',
  compare_dates: 'Comparaison de dates',
  list_alerts: 'Alertes',
}

/** Libellé lisible d'un tool MCP ; le nom brut s'il est inconnu. */
export function toolLabel(tool: string): string {
  return TOOL_LABELS[tool] ?? tool
}

/** Résumé du journal : « 12 appels, dont 1 en erreur ». */
export function callsSummary(calls: readonly McpCall[]): string {
  const failed = calls.filter((call) => !call.success).length
  const count = `${calls.length} appel${calls.length > 1 ? 's' : ''}`
  return failed === 0 ? count : `${count}, dont ${failed} en erreur`
}
