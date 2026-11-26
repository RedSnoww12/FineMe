import { describe, expect, it, vi } from 'vitest';
import { ApiUnreachableError, waitForApi } from './wake.ts';

describe('réveil de l\'API', () => {
  it('réessaie après une pause jusqu\'à ce que l\'API réponde', async () => {
    const fetch = vi
      .fn()
      .mockRejectedValueOnce(new Error('refusé'))
      .mockResolvedValueOnce({ ok: false })
      .mockResolvedValue({ ok: true });
    const sleep = vi.fn().mockResolvedValue(undefined);
    await waitForApi('https://api.example.test', { fetch, sleep });
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(fetch.mock.calls[0][0]).toBe('https://api.example.test/api/health');
    expect(sleep.mock.calls).toEqual([[30_000], [30_000]]);
  });

  it('abandonne après six essais, sans pause inutile à la fin', async () => {
    const fetch = vi.fn().mockResolvedValue({ ok: false });
    const sleep = vi.fn().mockResolvedValue(undefined);
    await expect(waitForApi('https://api.example.test', { fetch, sleep })).rejects.toThrow(ApiUnreachableError);
    expect(fetch).toHaveBeenCalledTimes(6);
    expect(sleep).toHaveBeenCalledTimes(5);
  });
});
