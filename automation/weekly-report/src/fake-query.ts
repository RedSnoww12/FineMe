// Faux `query` du SDK : rejoue un déroulé par appel et retient les paramètres reçus.

import type { SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { Query } from './agent.ts';

export function fakeQuery(...runs: unknown[][]) {
  const calls: Parameters<Query>[0][] = [];
  const query: Query = async function* (params) {
    calls.push(params);
    yield* (runs[Math.min(calls.length, runs.length) - 1] ?? []) as SDKMessage[];
  };
  return { query, calls };
}
