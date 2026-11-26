// Contrôle de forme du rapport avant envoi. Les messages d'erreur ne reprennent jamais son contenu : les journaux
// du dépôt sont publics.

export const SECTIONS = [
  'Synthèse',
  'Patrimoine et variation',
  'Performance face à l\'indice',
  'Budget face aux cibles',
  'Objectifs',
  'Alertes',
  'Points d\'attention',
] as const;

const SUMMARY_LINES = 3;
const MAX_LENGTH = 20_000;

const TITLE = /^# (Rapport de patrimoine — \d{2}\/\d{2}\/\d{4})\s*$/;

export type Report = { title: string; text: string };

export class InvalidReportError extends Error {}

function sectionsOf(lines: string[]): { heading: string; body: string[] }[] {
  const sections: { heading: string; body: string[] }[] = [];
  for (const line of lines) {
    if (line.startsWith('## ')) {
      sections.push({ heading: line.slice(3).trim(), body: [] });
    } else if (line.trim()) {
      sections.at(-1)?.body.push(line.trim());
    }
  }
  return sections;
}

/** Retient le rapport à partir de son titre et vérifie ses sections ; lève `InvalidReportError` sinon. */
export function parseReport(raw: string): Report {
  const lines = raw.split(/\r?\n/);
  const start = lines.findIndex((line) => TITLE.test(line));
  if (start < 0) {
    throw new InvalidReportError('Réponse sans titre de rapport.');
  }
  // Le modèle peut entourer le rapport d'un bloc de code : rien après sa clôture ne fait partie du rapport.
  const fence = lines.findIndex((line, index) => index > start && line.startsWith('```'));
  const body = lines.slice(start, fence < 0 ? undefined : fence);
  if (body.join('\n').length > MAX_LENGTH) {
    throw new InvalidReportError('Rapport anormalement long.');
  }
  const first = body.slice(1).find((line) => line.trim());
  if (!first?.startsWith('## ')) {
    throw new InvalidReportError('Texte inattendu entre le titre et la première section.');
  }
  const sections = sectionsOf(body.slice(1));
  const headings = sections.map((section) => section.heading);
  if (headings.join('\n') !== SECTIONS.join('\n')) {
    throw new InvalidReportError('Les sections du rapport ne suivent pas le format attendu.');
  }
  const empty = sections.find((section) => section.body.length === 0);
  if (empty) {
    throw new InvalidReportError(`Section « ${empty.heading} » vide.`);
  }
  const summary = sections[0].body;
  if (summary.length !== SUMMARY_LINES || !summary.every((line) => line.startsWith('- '))) {
    throw new InvalidReportError('La synthèse doit tenir en trois lignes.');
  }
  return { title: TITLE.exec(body[0])![1], text: body.join('\n').trim() };
}
