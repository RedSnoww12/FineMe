// Contenu des emails (texte brut : rien n'est interprété) et port d'envoi.

import type { Env } from './config.ts';
import type { Report } from './report.ts';

export type Email = { subject: string; text: string };

export interface Mailer {
  send(email: Email): Promise<void>;
}

export function reportEmail(report: Report): Email {
  return { subject: report.title, text: report.text };
}

export function unavailableEmail(reason: string, runUrl?: string): Email {
  const lines = [
    'Le rapport de patrimoine hebdomadaire n\'a pas pu être produit.',
    '',
    `Cause : ${reason}`,
  ];
  if (runUrl) {
    lines.push('', `Détail de l'exécution : ${runUrl}`);
  }
  return { subject: 'Rapport de patrimoine indisponible', text: lines.join('\n') };
}

export function redact(message: string, secrets: readonly string[]): string {
  return secrets.reduce((text, secret) => text.replaceAll(secret, '***'), message);
}

/** Adresse de l'exécution GitHub Actions en cours, absente en local. */
export function runUrl(env: Env): string | undefined {
  const { GITHUB_SERVER_URL: server, GITHUB_REPOSITORY: repository, GITHUB_RUN_ID: id } = env;
  return server && repository && id ? `${server}/${repository}/actions/runs/${id}` : undefined;
}
