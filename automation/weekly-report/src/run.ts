// Orchestration : configuration, réveil de l'API, rapport (deux essais), envoi. Tout échec finit par un email
// « rapport indisponible » et un code de sortie 1.

import { AgentError, runAgent, type Query } from './agent.ts';
import { ConfigError, readAgentConfig, readMailConfig, secretsOf, type Env, type MailConfig } from './config.ts';
import { redact, reportEmail, runUrl, unavailableEmail, type Mailer } from './mail.ts';
import { systemPromptFromSkill, USER_PROMPT } from './prompt.ts';
import { InvalidReportError, parseReport, type Report } from './report.ts';
import { ApiUnreachableError, waitForApi, type WakeDeps } from './wake.ts';

const ATTEMPTS = 2;
const RETRY_DELAY_MS = 60_000;
const AGENT_TIMEOUT_MS = 5 * 60_000;

export type Deps = WakeDeps & {
  query: Query;
  readSkill: () => string;
  mailer: (config: MailConfig) => Mailer;
  log: (message: string) => void;
};

/** Erreurs dont le message est écrit par nous : seules à pouvoir apparaître dans les journaux publics. */
function isKnown(error: unknown): error is Error {
  return [ConfigError, ApiUnreachableError, AgentError, InvalidReportError].some((type) => error instanceof type);
}

function nameOf(error: unknown): string {
  return error instanceof Error ? error.name : typeof error;
}

function publicReason(error: unknown): string {
  return isKnown(error) ? error.message : `Erreur inattendue (${nameOf(error)}).`;
}

/** Cause envoyée au seul destinataire : le détail d'une erreur imprévue y est utile, secrets masqués. */
function privateReason(error: unknown, secrets: string[]): string {
  const detail = !isKnown(error) && error instanceof Error ? ` ${error.message}` : '';
  return redact(`${publicReason(error)}${detail}`, secrets);
}

async function produceReport(env: Env, deps: Deps): Promise<Report> {
  const config = readAgentConfig(env);
  const systemPrompt = systemPromptFromSkill(deps.readSkill());
  await waitForApi(config.apiUrl, deps);
  for (let attempt = 1; ; attempt++) {
    try {
      const signal = AbortSignal.timeout(AGENT_TIMEOUT_MS);
      const raw = await runAgent(deps.query, { config, systemPrompt, prompt: USER_PROMPT, env, signal });
      return parseReport(raw);
    } catch (error) {
      if (attempt >= ATTEMPTS) throw error;
      deps.log(`Essai ${attempt} sur ${ATTEMPTS} : ${publicReason(error)}`);
      await deps.sleep(RETRY_DELAY_MS);
    }
  }
}

async function announceFailure(
  env: Env,
  deps: Deps,
  mailer: Mailer,
  logged: string,
  emailed: string,
): Promise<number> {
  deps.log(`Rapport indisponible : ${logged}`);
  try {
    await mailer.send(unavailableEmail(emailed, runUrl(env)));
  } catch (error) {
    deps.log(`Email d'échec non envoyé (${nameOf(error)}).`);
  }
  return 1;
}

/** Renvoie le code de sortie du processus. */
export async function runWeeklyReport(env: Env, deps: Deps): Promise<number> {
  let mailer: Mailer;
  try {
    mailer = deps.mailer(readMailConfig(env));
  } catch (error) {
    deps.log(`Envoi impossible : ${publicReason(error)}`);
    return 1;
  }
  let report: Report;
  try {
    report = await produceReport(env, deps);
  } catch (error) {
    return announceFailure(env, deps, mailer, publicReason(error), privateReason(error, secretsOf(env)));
  }
  try {
    await mailer.send(reportEmail(report));
  } catch (error) {
    const reason = 'Le rapport a été produit mais son envoi a échoué.';
    return announceFailure(env, deps, mailer, `${reason} (${nameOf(error)})`, `${reason} ${privateReason(error, secretsOf(env))}`);
  }
  deps.log('Rapport envoyé.');
  return 0;
}
