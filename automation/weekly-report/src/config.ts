// Lecture et validation de l'environnement. Les messages d'erreur nomment les variables, jamais leurs valeurs.

export type Env = Record<string, string | undefined>;

export type MailConfig = {
  host: string;
  port: number;
  user: string;
  password: string;
  from: string;
  to: string;
};

export type AgentConfig = {
  /** URL de l'API sans barre finale ni /api. */
  apiUrl: string;
  mcpToken: string;
  oauthToken: string;
  model: string;
};

export const DEFAULT_MODEL = 'claude-opus-5-5';

const LOCAL_HOSTS = ['localhost', '127.0.0.1', '[::1]'];

export class ConfigError extends Error {}

function required<K extends string>(env: Env, names: readonly K[]): Record<K, string> {
  const missing = names.filter((name) => !env[name]?.trim());
  if (missing.length > 0) {
    throw new ConfigError(`Variables d'environnement manquantes : ${missing.join(', ')}.`);
  }
  return Object.fromEntries(names.map((name) => [name, env[name]!.trim()])) as Record<K, string>;
}

function smtpPort(value: string): number {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new ConfigError('SMTP_PORT doit être un numéro de port.');
  }
  return port;
}

/** Le jeton d'accès voyage avec chaque requête : https obligatoire hors de la machine locale. */
function apiUrl(value: string): string {
  const url = URL.parse(value);
  const secure = url?.protocol === 'https:' || (url?.protocol === 'http:' && LOCAL_HOSTS.includes(url.hostname));
  if (!url || !secure || url.username || url.password || url.search || url.hash) {
    throw new ConfigError('FINEME_API_URL doit être une adresse https, sans identifiants ni paramètres.');
  }
  // Reconstruite pour écarter un « ? » ou un « # » vide ; un /api final est toléré et retiré.
  return `${url.origin}${url.pathname}`.replace(/\/+$/, '').replace(/\/api$/, '');
}

export function readMailConfig(env: Env): MailConfig {
  const values = required(env, ['SMTP_HOST', 'SMTP_PORT', 'SMTP_USER', 'SMTP_PASSWORD', 'REPORT_EMAIL_TO']);
  return {
    host: values.SMTP_HOST,
    port: smtpPort(values.SMTP_PORT),
    user: values.SMTP_USER,
    password: values.SMTP_PASSWORD,
    from: env.REPORT_EMAIL_FROM?.trim() || values.SMTP_USER,
    to: values.REPORT_EMAIL_TO,
  };
}

export function readAgentConfig(env: Env): AgentConfig {
  const values = required(env, ['FINEME_API_URL', 'FINEME_MCP_TOKEN', 'CLAUDE_CODE_OAUTH_TOKEN']);
  if (!values.FINEME_MCP_TOKEN.startsWith('fm_pat_')) {
    throw new ConfigError('FINEME_MCP_TOKEN doit être un jeton d\'accès FineMe (Paramètres > Accès IA).');
  }
  return {
    apiUrl: apiUrl(values.FINEME_API_URL),
    mcpToken: values.FINEME_MCP_TOKEN,
    oauthToken: values.CLAUDE_CODE_OAUTH_TOKEN,
    model: env.REPORT_MODEL?.trim() || DEFAULT_MODEL,
  };
}

/** Valeurs à ne jamais laisser sortir dans un email ou un journal. */
export function secretsOf(env: Env): string[] {
  return ['FINEME_MCP_TOKEN', 'CLAUDE_CODE_OAUTH_TOKEN', 'SMTP_PASSWORD']
    .map((name) => env[name]?.trim() ?? '')
    .filter((value) => value.length > 0);
}
