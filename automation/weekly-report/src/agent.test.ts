import { describe, expect, it } from 'vitest';
import { AgentError, agentOptions, MCP_TOOLS, runAgent, type AgentRequest } from './agent.ts';
import { readAgentConfig } from './config.ts';
import { fakeQuery } from './fake-query.ts';
import { ENV, REPORT, transcript } from './testing.ts';

function request(): AgentRequest {
  return {
    config: readAgentConfig(ENV),
    systemPrompt: 'format',
    prompt: 'rapport',
    env: { ...ENV, PATH: '/usr/bin', HOME: '/home/runner' },
    signal: new AbortController().signal,
  };
}

describe('options de l\'agent', () => {
  it('n\'autorise que les six tools FineMe et aucun outil intégré', () => {
    const options = agentOptions(request());
    expect(options.tools).toEqual([]);
    expect(options.allowedTools).toEqual(MCP_TOOLS);
    expect(MCP_TOOLS).toHaveLength(6);
    expect(options.permissionMode).toBe('dontAsk');
    expect(options.strictMcpConfig).toBe(true);
    expect(options.settingSources).toEqual([]);
    expect(options.persistSession).toBe(false);
    expect(options.mcpServers).toEqual({
      fineme: {
        type: 'http',
        url: 'https://api.example.test/mcp',
        headers: { Authorization: 'Bearer fm_pat_fictif' },
        alwaysLoad: true,
      },
    });
  });

  it('ne transmet au sous-processus ni le mot de passe SMTP ni le jeton FineMe', () => {
    expect(agentOptions(request()).env).toEqual({
      PATH: '/usr/bin',
      HOME: '/home/runner',
      CLAUDE_CODE_OAUTH_TOKEN: 'oauth-fictif',
    });
  });

  it('propage l\'interruption', () => {
    const controller = new AbortController();
    const options = agentOptions({ ...request(), signal: controller.signal });
    controller.abort();
    expect(options.abortController?.signal.aborted).toBe(true);
  });
});

describe('déroulé de l\'agent', () => {
  const run = (messages: unknown[]) => runAgent(fakeQuery(messages).query, request());

  it('rend le texte final', async () => {
    await expect(run(transcript())).resolves.toBe(REPORT);
  });

  it('signale une erreur de l\'API Claude, dont le sous-type reste « success »', async () => {
    const messages = [...transcript().slice(0, -1), { type: 'result', subtype: 'success', is_error: true, result: 'x' }];
    await expect(run(messages)).rejects.toThrow('API Claude');
  });

  it('signale le délai dépassé', async () => {
    const controller = new AbortController();
    const query = async function* () {
      controller.abort();
      yield* [];
      throw new Error('aborted');
    };
    await expect(runAgent(query, { ...request(), signal: controller.signal })).rejects.toThrow('délai');
  });

  it.each([
    ['le serveur MCP n\'est pas connecté', transcript({ status: 'failed' }), 'injoignable'],
    ['un autre outil est disponible', transcript({ tools: ['Bash'] }), 'outil étranger'],
    ['la vue d\'ensemble échoue', transcript({ overviewError: true }), 'vue d\'ensemble'],
    ['la vue d\'ensemble n\'est pas lue', transcript({ skipOverview: true }), 'vue d\'ensemble'],
    ['le nombre de tours est dépassé', transcript({ subtype: 'error_max_turns' }), 'error_max_turns'],
    ['aucun résultat n\'arrive', transcript().slice(0, 1), 'sans résultat'],
  ])('échoue quand %s', async (_, messages, expected) => {
    await expect(run(messages)).rejects.toThrow(AgentError);
    await expect(run(messages)).rejects.toThrow(expected);
  });
});
