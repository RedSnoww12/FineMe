import { describe, expect, it } from 'vitest';
import { readMailConfig } from './config.ts';
import { redact, reportEmail, runUrl, unavailableEmail } from './mail.ts';
import { smtpOptions } from './smtp.ts';
import { ENV } from './testing.ts';

describe('emails', () => {
  it('envoie le rapport avec son titre pour sujet', () => {
    expect(reportEmail({ title: 'Titre', text: 'Corps' })).toEqual({ subject: 'Titre', text: 'Corps' });
  });

  it('annonce un rapport indisponible, avec la cause et le lien de l\'exécution', () => {
    const email = unavailableEmail('Serveur MCP injoignable.', 'https://github.example.test/run/1');
    expect(email.subject).toBe('Rapport de patrimoine indisponible');
    expect(email.text).toContain('Serveur MCP injoignable.');
    expect(email.text).toContain('https://github.example.test/run/1');
    expect(unavailableEmail('x').text).not.toContain('exécution :');
  });

  it('désamorce les liens du rapport et de la cause, tronquée, mais pas celui de l\'exécution', () => {
    expect(reportEmail({ title: 'T', text: 'voir https://evil.example/x' }).text).toBe('voir [lien retiré]');
    const { text } = unavailableEmail(`https://evil.example ${'a'.repeat(2000)}`, 'https://github.example.test/run/1');
    expect(text).not.toContain('evil');
    expect(text.length).toBeLessThan(800);
    expect(text).toContain('https://github.example.test/run/1');
  });

  it('masque les secrets', () => {
    expect(redact('refus de fm_pat_x puis fm_pat_x', ['fm_pat_x'])).toBe('refus de *** puis ***');
  });

  it('construit l\'adresse de l\'exécution GitHub Actions', () => {
    const env = { GITHUB_SERVER_URL: 'https://github.com', GITHUB_REPOSITORY: 'a/b', GITHUB_RUN_ID: '7' };
    expect(runUrl(env)).toBe('https://github.com/a/b/actions/runs/7');
    expect(runUrl({})).toBeUndefined();
  });

  it('impose TLS quel que soit le port', () => {
    const config = readMailConfig(ENV);
    expect(smtpOptions(config)).toMatchObject({ secure: false, requireTLS: true });
    expect(smtpOptions({ ...config, port: 465 })).toMatchObject({ secure: true, requireTLS: false });
  });
});
