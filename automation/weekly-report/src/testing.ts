// Fixtures fictives partagées par les tests.

export const ENV = {
  FINEME_API_URL: 'https://api.example.test/',
  FINEME_MCP_TOKEN: 'fm_pat_fictif',
  CLAUDE_CODE_OAUTH_TOKEN: 'oauth-fictif',
  SMTP_HOST: 'smtp.example.test',
  SMTP_PORT: '587',
  SMTP_USER: 'rapport@example.test',
  SMTP_PASSWORD: 'mot-de-passe-fictif',
  REPORT_EMAIL_TO: 'foyer@example.test',
};

export const REPORT = `# Rapport de patrimoine — 27/11/2026

## Synthèse
- Patrimoine net : 1 000 €.
- Performance : indisponible.
- Aucun point d'attention.

## Patrimoine et variation
- Patrimoine net : 1 000 €.

## Performance face à l'indice
- Aucun indice de référence choisi dans FineMe.

## Budget face aux cibles
- Aucune cible de budget définie.

## Objectifs
Aucun objectif.

## Alertes
Aucune alerte en cours.

## Points d'attention
Rien à signaler.`;

type Init = { status?: string; tools?: string[] };

/** Déroulé d'agent fictif : initialisation, appel de get_overview, résultat. */
export function transcript(
  options: Init & { result?: string; overviewError?: boolean; subtype?: string; skipOverview?: boolean } = {},
): unknown[] {
  const { status = 'connected', tools = ['mcp__fineme__get_overview'], subtype = 'success' } = options;
  const overview = [
    { type: 'assistant', message: { content: [{ type: 'tool_use', id: 't1', name: 'mcp__fineme__get_overview' }] } },
    {
      type: 'user',
      message: { content: [{ type: 'tool_result', tool_use_id: 't1', is_error: options.overviewError ?? false }] },
    },
  ];
  return [
    { type: 'system', subtype: 'init', mcp_servers: [{ name: 'fineme', status }], tools },
    ...(options.skipOverview ? [] : overview),
    { type: 'result', subtype, is_error: subtype !== 'success', result: options.result ?? REPORT },
  ];
}
