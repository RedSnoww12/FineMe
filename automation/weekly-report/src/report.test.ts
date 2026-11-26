import { describe, expect, it } from 'vitest';
import { InvalidReportError, parseReport } from './report.ts';
import { REPORT } from './testing.ts';

describe('contrôle du rapport', () => {
  it('accepte le format attendu et en tire le sujet', () => {
    expect(parseReport(REPORT)).toEqual({ title: 'Rapport de patrimoine — 27/11/2026', text: REPORT });
  });

  it('écarte ce qui précède le titre et la clôture d\'un bloc de code', () => {
    expect(parseReport(`Voici le rapport :\n\n\`\`\`markdown\n${REPORT}\n\`\`\``).text).toBe(REPORT);
  });

  it('écarte tout texte ajouté après la clôture du bloc de code', () => {
    expect(parseReport(`\`\`\`\n${REPORT}\n\`\`\`\n\nSouhaitez-vous un détail ?`).text).toBe(REPORT);
  });

  it.each([
    ['sans titre', REPORT.replace('# Rapport de patrimoine —', '# Bilan')],
    ['sans une section', REPORT.replace('## Objectifs\nAucun objectif.\n', '')],
    ['avec une section en trop', `${REPORT}\n\n## Conseils\nAcheter.`],
    ['avec une section vide', REPORT.replace('Aucun objectif.', '')],
    ['avec une synthèse de deux lignes', REPORT.replace('- Performance : indisponible.\n', '')],
    ['avec une synthèse sans puces', REPORT.replace('- Performance', 'Performance')],
    ['avec du texte avant la première section', REPORT.replace('## Synthèse', 'Reconnectez-vous.\n\n## Synthèse')],
    ['anormalement long', `${REPORT}\n${'x'.repeat(20_000)}`],
    ['vide', ''],
  ])('refuse un rapport %s', (_, raw) => {
    expect(() => parseReport(raw)).toThrow(InvalidReportError);
  });

  it('ne recopie pas le contenu dans son message d\'erreur', () => {
    const read = () => parseReport(REPORT.replace('## Alertes', '## Montant 123456 €'));
    expect(read).toThrow(InvalidReportError);
    expect(read).not.toThrow('123456');
  });
});
