// Point d'entrée : branche les vraies dépendances.

import { readFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
import { query } from '@anthropic-ai/claude-agent-sdk';
import { SKILL_URL } from './prompt.ts';
import { runWeeklyReport } from './run.ts';
import { smtpMailer } from './smtp.ts';

// Filet : Node écrirait sinon le message et la pile d'une erreur non attrapée dans des journaux publics.
for (const event of ['uncaughtException', 'unhandledRejection'] as const) {
  process.on(event, (error: unknown) => {
    console.log(`Arrêt sur une erreur non attrapée (${error instanceof Error ? error.name : typeof error}).`);
    process.exit(1);
  });
}

process.exitCode = await runWeeklyReport(process.env, {
  query,
  readSkill: () => readFileSync(SKILL_URL, 'utf8'),
  mailer: smtpMailer,
  fetch,
  sleep: (ms) => setTimeout(ms),
  log: (message) => console.log(message),
});
