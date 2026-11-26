import { describe, expect, it, vi } from 'vitest';
import type { Email } from './mail.ts';
import { fakeQuery } from './fake-query.ts';
import { runWeeklyReport, type Deps } from './run.ts';
import { ENV, REPORT, transcript } from './testing.ts';

const SKILL = '---\nname: rapport-patrimoine\n---\n# Format\n';

function setup(runs: unknown[][], overrides: Partial<Deps> = {}) {
  const sent: Email[] = [];
  const logs: string[] = [];
  const { query, calls } = fakeQuery(...runs);
  const deps: Deps = {
    query,
    readSkill: () => SKILL,
    mailer: () => ({ send: async (email) => void sent.push(email) }),
    fetch: async () => ({ ok: true }),
    sleep: vi.fn().mockResolvedValue(undefined),
    log: (message) => void logs.push(message),
    ...overrides,
  };
  return { deps, sent, logs, calls };
}

describe('rapport hebdomadaire', () => {
  it('envoie le rapport', async () => {
    const { deps, sent, calls } = setup([transcript()]);
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(0);
    expect(sent).toEqual([{ subject: 'Rapport de patrimoine — 27/11/2026', text: REPORT }]);
    expect(calls).toHaveLength(1);
    expect(calls[0].options.systemPrompt).toBe('# Format');
  });

  it('fait un second essai après une pause', async () => {
    const { deps, sent, calls } = setup([transcript({ status: 'failed' }), transcript()]);
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(0);
    expect(calls).toHaveLength(2);
    expect(deps.sleep).toHaveBeenCalledWith(60_000);
    expect(sent[0].subject).toContain('Rapport de patrimoine —');
  });

  it('annonce le rapport indisponible après deux réponses mal formées, sans les recopier', async () => {
    const { deps, sent, logs, calls } = setup([transcript({ result: 'Solde 123456 €' })]);
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(1);
    expect(calls).toHaveLength(2);
    expect(sent.map((email) => email.subject)).toEqual(['Rapport de patrimoine indisponible']);
    expect(`${sent[0].text}${logs.join()}`).not.toContain('123456');
  });

  it('n\'appelle pas l\'agent quand l\'API ne répond pas', async () => {
    const { deps, sent, calls } = setup([transcript()], { fetch: async () => ({ ok: false }) });
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(1);
    expect(calls).toHaveLength(0);
    expect(sent[0].text).toContain('ne répond pas');
  });

  it('prévient par email quand la configuration de l\'agent manque', async () => {
    const { deps, sent } = setup([transcript()]);
    await expect(runWeeklyReport({ ...ENV, FINEME_MCP_TOKEN: '' }, deps)).resolves.toBe(1);
    expect(sent[0].text).toContain('FINEME_MCP_TOKEN');
  });

  it('garde le détail d\'une erreur imprévue hors des journaux, secrets masqués dans l\'email', async () => {
    const query: Deps['query'] = () => {
      throw new Error('refus du jeton oauth-fictif, détail sensible');
    };
    const { deps, sent, logs } = setup([], { query });
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(1);
    expect(sent[0].text).toContain('détail sensible');
    expect(sent[0].text).not.toContain('oauth-fictif');
    expect(logs.join()).not.toContain('sensible');
  });

  it('s\'arrête sans appeler l\'agent quand l\'envoi n\'est pas configuré', async () => {
    const { deps, logs, calls } = setup([transcript()]);
    await expect(runWeeklyReport({}, deps)).resolves.toBe(1);
    expect(calls).toHaveLength(0);
    expect(logs.join()).toContain('SMTP_HOST');
  });

  it('dit que l\'envoi a échoué quand le rapport était produit', async () => {
    const sent: Email[] = [];
    const send = vi
      .fn()
      .mockRejectedValueOnce(new Error('SMTP occupé'))
      .mockImplementation(async (email: Email) => void sent.push(email));
    const { deps, logs } = setup([transcript()], { mailer: () => ({ send }) });
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(1);
    expect(sent[0].text).toContain('produit mais son envoi a échoué');
    expect(logs.join()).not.toContain('SMTP occupé');
  });

  it('échoue proprement quand aucun email ne part', async () => {
    const mailer: Deps['mailer'] = () => ({ send: async () => Promise.reject(new Error('SMTP refusé')) });
    const { deps, logs } = setup([transcript()], { mailer });
    await expect(runWeeklyReport(ENV, deps)).resolves.toBe(1);
    expect(logs.at(-1)).toContain('non envoyé');
  });
});
