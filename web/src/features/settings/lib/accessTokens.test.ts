import { describe, expect, it } from 'vitest'
import type { AccessToken, McpCall } from '@/shared/api/types'
import { activeCount, callsSummary, claudeAddCommand, isActive, mcpEndpoint, toolLabel } from './accessTokens'

const token = (overrides: Partial<AccessToken> = {}): AccessToken => ({
  id: 't1',
  name: 'Claude Code',
  scope: 'read',
  hint: 'fm_pat_…abcd',
  created_at: '2026-11-01T10:00:00Z',
  last_used_at: null,
  revoked_at: null,
  ...overrides,
})

const call = (success: boolean): McpCall => ({
  at: '2026-11-02T10:00:00Z',
  token_id: 't1',
  token_name: 'Claude Code',
  tool: 'get_overview',
  success,
})

describe('adresse du serveur MCP', () => {
  it('prend l’API si elle est sur un autre domaine, sinon l’origine de la page', () => {
    expect(mcpEndpoint('https://api.exemple.test/', 'https://app.exemple.test')).toBe('https://api.exemple.test/mcp')
    expect(mcpEndpoint('', 'http://localhost:5173')).toBe('http://localhost:5173/mcp')
  })

  it('compose la commande Claude Code avec le jeton', () => {
    expect(claudeAddCommand('http://localhost:5173/mcp', 'fm_pat_x')).toBe(
      'claude mcp add --transport http fineme http://localhost:5173/mcp --header "Authorization: Bearer fm_pat_x"',
    )
  })
})

describe('jetons et journal', () => {
  it('compte les jetons actifs seulement', () => {
    const revoked = token({ id: 't2', revoked_at: '2026-11-03T10:00:00Z' })
    expect(isActive(revoked)).toBe(false)
    expect(activeCount([token(), revoked])).toBe(1)
  })

  it('traduit les tools connus et garde les autres tels quels', () => {
    expect(toolLabel('compare_dates')).toBe('Comparaison de dates')
    expect(toolLabel('outil_futur')).toBe('outil_futur')
  })

  it('résume le journal avec les erreurs', () => {
    expect(callsSummary([call(true)])).toBe('1 appel')
    expect(callsSummary([call(true), call(false), call(true)])).toBe('3 appels, dont 1 en erreur')
  })
})
