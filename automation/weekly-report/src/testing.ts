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
