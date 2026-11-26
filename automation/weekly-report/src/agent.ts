// Appel du Claude Agent SDK, enfermé : aucun outil intégré, seuls les six tools en lecture du serveur MCP FineMe.
// Les messages d'erreur sont fixes (journaux publics) : jamais le texte du modèle ni celui des tools.

import type { Options, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { AgentConfig, Env } from './config.ts';

const SERVER = 'fineme';
const PREFIX = `mcp__${SERVER}__`;

export const MCP_TOOLS = [
  'get_overview',
  'compare_dates',
  'get_performance',
  'get_budget',
  'get_goals',
  'list_alerts',
].map((tool) => `${PREFIX}${tool}`);

const OVERVIEW = `${PREFIX}get_overview`;
const MAX_TURNS = 12;
const MAX_BUDGET_USD = 2;

/** Variables transmises au sous-processus : ni le mot de passe SMTP, ni le reste de l'environnement. */
const INHERITED = ['PATH', 'HOME', 'USER', 'TMPDIR', 'LANG'];

export class AgentError extends Error {}

export type Query = (params: { prompt: string; options: Options }) => AsyncIterable<SDKMessage>;

export type AgentRequest = {
  config: AgentConfig;
  systemPrompt: string;
  prompt: string;
  env: Env;
  signal: AbortSignal;
};

export function agentOptions({ config, systemPrompt, env, signal }: AgentRequest): Options {
  const abortController = new AbortController();
  signal.addEventListener('abort', () => abortController.abort(signal.reason), { once: true });
  return {
    abortController,
    model: config.model,
    systemPrompt,
    tools: [],
    allowedTools: MCP_TOOLS,
    permissionMode: 'dontAsk',
    mcpServers: {
      [SERVER]: {
        type: 'http',
        url: `${config.apiUrl}/mcp`,
        headers: { Authorization: `Bearer ${config.mcpToken}` },
        alwaysLoad: true,
      },
    },
    strictMcpConfig: true,
    settingSources: [],
    persistSession: false,
    maxTurns: MAX_TURNS,
    maxBudgetUsd: MAX_BUDGET_USD,
    env: {
      ...Object.fromEntries(INHERITED.flatMap((name) => (env[name] ? [[name, env[name]]] : []))),
      CLAUDE_CODE_OAUTH_TOKEN: config.oauthToken,
    },
  };
}

function checkInit(message: Extract<SDKMessage, { type: 'system'; subtype: 'init' }>): void {
  const server = message.mcp_servers.find((candidate) => candidate.name === SERVER);
  if (server?.status !== 'connected') {
    throw new AgentError('Serveur MCP FineMe injoignable ou jeton d\'accès refusé.');
  }
  if (!message.tools.every((tool) => tool.startsWith(PREFIX))) {
    throw new AgentError('Un outil étranger à FineMe est disponible : exécution arrêtée.');
  }
}

function blocksOf(content: unknown): Record<string, unknown>[] {
  return Array.isArray(content) ? content : [];
}

/** Texte final de l'agent, accepté seulement si la vue d'ensemble a bien été lue. */
export async function runAgent(query: Query, request: AgentRequest): Promise<string> {
  try {
    return await readReport(query({ prompt: request.prompt, options: agentOptions(request) }));
  } catch (error) {
    if (request.signal.aborted) throw new AgentError('L\'agent a dépassé le délai imparti.');
    throw error;
  }
}

async function readReport(messages: AsyncIterable<SDKMessage>): Promise<string> {
  const overviewCalls = new Set<unknown>();
  let overviewRead = false;
  for await (const message of messages) {
    if (message.type === 'system' && message.subtype === 'init') {
      checkInit(message);
    } else if (message.type === 'assistant') {
      for (const block of blocksOf(message.message.content)) {
        if (block.type === 'tool_use' && block.name === OVERVIEW) overviewCalls.add(block.id);
      }
    } else if (message.type === 'user') {
      for (const block of blocksOf(message.message.content)) {
        if (block.type === 'tool_result' && overviewCalls.has(block.tool_use_id) && !block.is_error) {
          overviewRead = true;
        }
      }
    } else if (message.type === 'result') {
      if (message.subtype !== 'success') {
        throw new AgentError(`L'agent s'est arrêté sans rapport (${message.subtype}).`);
      }
      // Erreur de l'API Claude (jeton expiré, surcharge) : le sous-type reste « success ».
      if (message.is_error) {
        throw new AgentError('L\'API Claude a renvoyé une erreur (jeton expiré ou service indisponible).');
      }
      if (!overviewRead) {
        throw new AgentError('La vue d\'ensemble du patrimoine n\'a pas pu être lue.');
      }
      return message.result;
    }
  }
  throw new AgentError('L\'agent s\'est arrêté sans résultat.');
}
