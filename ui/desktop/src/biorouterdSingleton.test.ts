import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { BiorouterdResult, StartBiorouterdOptions } from './biorouterd';
import { getSharedBackend, resetSharedBackend, isSharedDaemonEnabled } from './biorouterdSingleton';

const fakeResult = (baseUrl: string): BiorouterdResult => ({
  baseUrl,
  managed: true,
  workingDir: '/home/tester',
  // A ChildProcess stand-in — the singleton never touches it.
  process: { kill: () => {} } as unknown as BiorouterdResult['process'],
  errorLog: [],
});

const opts = {} as StartBiorouterdOptions;

describe('getSharedBackend', () => {
  beforeEach(() => resetSharedBackend());

  it('starts the backend exactly once across N concurrent calls', async () => {
    const start = vi.fn(async () => fakeResult('http://127.0.0.1:5001'));

    const [a, b, c] = await Promise.all([
      getSharedBackend(start, opts),
      getSharedBackend(start, opts),
      getSharedBackend(start, opts),
    ]);

    expect(start).toHaveBeenCalledTimes(1);
    // All windows get the same daemon result.
    expect(a).toBe(b);
    expect(b).toBe(c);
    expect(a.baseUrl).toBe('http://127.0.0.1:5001');
  });

  it('reuses the same backend across sequential calls', async () => {
    const start = vi.fn(async () => fakeResult('http://127.0.0.1:5002'));

    const first = await getSharedBackend(start, opts);
    const second = await getSharedBackend(start, opts);

    expect(start).toHaveBeenCalledTimes(1);
    expect(first).toBe(second);
  });

  it('re-initializes after reset', async () => {
    const start = vi
      .fn<(o: StartBiorouterdOptions) => Promise<BiorouterdResult>>()
      .mockResolvedValueOnce(fakeResult('http://127.0.0.1:5003'))
      .mockResolvedValueOnce(fakeResult('http://127.0.0.1:5004'));

    const first = await getSharedBackend(start, opts);
    resetSharedBackend();
    const second = await getSharedBackend(start, opts);

    expect(start).toHaveBeenCalledTimes(2);
    expect(first.baseUrl).toBe('http://127.0.0.1:5003');
    expect(second.baseUrl).toBe('http://127.0.0.1:5004');
  });

  it('does not cache a rejected start (a later window can retry)', async () => {
    const start = vi
      .fn<(o: StartBiorouterdOptions) => Promise<BiorouterdResult>>()
      .mockRejectedValueOnce(new Error('spawn failed'))
      .mockResolvedValueOnce(fakeResult('http://127.0.0.1:5005'));

    await expect(getSharedBackend(start, opts)).rejects.toThrow('spawn failed');
    // Retry succeeds because the failed promise was not cached.
    const ok = await getSharedBackend(start, opts);

    expect(start).toHaveBeenCalledTimes(2);
    expect(ok.baseUrl).toBe('http://127.0.0.1:5005');
  });
});

describe('isSharedDaemonEnabled', () => {
  it('defaults to on when unset', () => {
    expect(isSharedDaemonEnabled({})).toBe(true);
  });

  it.each(['0', 'false', 'FALSE', 'off', 'No', ' false '])(
    'is off when BIOROUTER_SHARED_DAEMON=%s',
    (flag) => {
      expect(isSharedDaemonEnabled({ BIOROUTER_SHARED_DAEMON: flag })).toBe(false);
    }
  );

  it.each(['1', 'true', 'on', 'yes', 'anything'])(
    'is on when BIOROUTER_SHARED_DAEMON=%s',
    (flag) => {
      expect(isSharedDaemonEnabled({ BIOROUTER_SHARED_DAEMON: flag })).toBe(true);
    }
  );
});
