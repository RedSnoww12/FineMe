// Contenu des emails (texte brut : rien n'est interprété) et port d'envoi.

import type { Env } from './config.ts';
import { defangLinks } from './links.ts';
import type { Report } from './report.ts';

const MAX_REASON_LENGTH = 500;

export type Email = { subject: string; text: string };

export interface Mailer {
  send(email: Email): Promise<void>;
}

export function reportEmail(report: Report): Email {
  return { subject: report.title, text: defangLinks(report.text) };
}

export function unavailableEmail(reason: string, runUrl?: string): Email {
  const lines = [
    'Le rapport de patrimoine hebdomadaire n\'est pas disponible.',
    '',
    // La cause peut reprendre le message d'une erreur imprévue ; l'adresse de l'exécution, elle, est la nôtre.
    `Cause : ${defangLinks(reason.slice(0, 4 * MAX_REASON_LENGTH)).slice(0, MAX_REASON_LENGTH)}`,
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
