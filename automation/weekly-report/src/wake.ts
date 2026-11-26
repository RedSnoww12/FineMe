// L'offre gratuite de Render met l'API en veille : on attend qu'elle réponde avant de lancer l'agent.

const ATTEMPTS = 6;
const DELAY_MS = 30_000;
const REQUEST_TIMEOUT_MS = 60_000;

export class ApiUnreachableError extends Error {}

export type WakeDeps = {
  fetch: (url: string, init: RequestInit) => Promise<{ ok: boolean }>;
  sleep: (ms: number) => Promise<void>;
};

export async function waitForApi(apiUrl: string, { fetch, sleep }: WakeDeps): Promise<void> {
  for (let attempt = 1; attempt <= ATTEMPTS; attempt++) {
    try {
      const response = await fetch(`${apiUrl}/api/health`, { signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
      if (response.ok) return;
    } catch {
      // Connexion refusée ou délai dépassé : l'instance démarre peut-être encore.
    }
    if (attempt < ATTEMPTS) await sleep(DELAY_MS);
  }
  throw new ApiUnreachableError(`L'API FineMe ne répond pas après ${ATTEMPTS} essais.`);
}
