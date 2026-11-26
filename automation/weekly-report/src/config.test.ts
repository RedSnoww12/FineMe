import { describe, expect, it } from 'vitest';
import { ConfigError, DEFAULT_MODEL, readAgentConfig, readMailConfig, secretsOf } from './config.ts';
import { ENV } from './testing.ts';

describe('configuration', () => {
  it('lit l\'envoi, avec l\'utilisateur SMTP comme expéditeur par défaut', () => {
    expect(readMailConfig(ENV)).toMatchObject({ port: 587, from: ENV.SMTP_USER, to: ENV.REPORT_EMAIL_TO });
    expect(readMailConfig({ ...ENV, REPORT_EMAIL_FROM: 'moi@example.test' }).from).toBe('moi@example.test');
  });

  it('retire la barre finale de l\'URL et choisit le modèle par défaut', () => {
    expect(readAgentConfig(ENV)).toMatchObject({ apiUrl: 'https://api.example.test', model: DEFAULT_MODEL });
    expect(readAgentConfig({ ...ENV, REPORT_MODEL: 'claude-sonnet-5-5' }).model).toBe('claude-sonnet-5-5');
  });

  it('nomme toutes les variables manquantes, sans aucune valeur', () => {
    const read = () => readMailConfig({ ...ENV, SMTP_HOST: ' ', SMTP_PASSWORD: undefined });
    expect(read).toThrow(ConfigError);
    expect(read).toThrow('SMTP_HOST, SMTP_PASSWORD');
  });

  it.each(['0', 'abc', '70000'])('refuse le port %s', (port) => {
    expect(() => readMailConfig({ ...ENV, SMTP_PORT: port })).toThrow('SMTP_PORT');
  });

  it.each(['https://api.example.test/api/', 'https://api.example.test/?', 'https://api.example.test/#'])(
    'ramène %s à la racine de l\'API',
    (url) => {
      expect(readAgentConfig({ ...ENV, FINEME_API_URL: url }).apiUrl).toBe('https://api.example.test');
    },
  );

  it.each([
    'http://api.example.test',
    'api.example.test',
    'https://user:secret@api.example.test',
    'https://api.example.test/?a=1',
  ])('refuse l\'URL %s', (url) => {
    expect(() => readAgentConfig({ ...ENV, FINEME_API_URL: url })).toThrow('FINEME_API_URL');
  });

  it('accepte http sur la machine locale', () => {
    expect(readAgentConfig({ ...ENV, FINEME_API_URL: 'http://localhost:8080' }).apiUrl).toBe('http://localhost:8080');
  });

  it('refuse autre chose qu\'un jeton d\'accès, sans le citer', () => {
    const read = () => readAgentConfig({ ...ENV, FINEME_MCP_TOKEN: 'eyJ-jwt-de-session' });
    expect(read).toThrow('FINEME_MCP_TOKEN');
    expect(read).not.toThrow('eyJ');
  });

  it('liste les secrets présents', () => {
    expect(secretsOf(ENV)).toEqual(['fm_pat_fictif', 'oauth-fictif', 'mot-de-passe-fictif']);
    expect(secretsOf({})).toEqual([]);
  });
});
