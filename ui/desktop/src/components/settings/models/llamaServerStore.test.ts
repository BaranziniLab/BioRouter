import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const mockLlamacppStatus = vi.fn();

vi.mock('../../../api', () => ({
  llamacppStatus: (...args: unknown[]) => mockLlamacppStatus(...args),
}));

import {
  llamaServerStore,
  resetLlamaServerStoreForTests,
  LLAMA_SERVER_POLL_INTERVAL_MS,
  LLAMA_SERVER_OPERATION_TIMEOUT_MS,
} from './llamaServerStore';

const statusResponse = (sidecar: Record<string, unknown>) => ({
  data: {
    sidecar: {
      state: 'starting',
      warmed: false,
      build: 'test',
      detail: null,
      model: 'gemma4',
      ...sidecar,
    },
    catalog: [],
    system: {
      os: 'macos',
      total_memory_gib: 64,
      accelerator_memory_gib: 64,
      accelerator_memory_kind: 'apple_unified',
      default_context_size: 131072,
      model_cache_dir: '/tmp/models',
      model_cache_layout: 'test',
    },
  },
});

const advanceTicks = async (ticks: number) => {
  await vi.advanceTimersByTimeAsync(ticks * LLAMA_SERVER_POLL_INTERVAL_MS);
};

describe('llamaServerStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetLlamaServerStoreForTests();
    mockLlamacppStatus.mockReset();
    mockLlamacppStatus.mockResolvedValue(statusResponse({ state: 'starting' }));
  });

  afterEach(() => {
    resetLlamaServerStoreForTests();
    vi.useRealTimers();
  });

  it('polls while an operation is in flight, even with zero subscribers', async () => {
    llamaServerStore.beginOperation('install', 'gemma4', 'Preparing install...');

    await advanceTicks(3);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(3);
    expect(llamaServerStore.getSnapshot().operation).toMatchObject({
      kind: 'install',
      model: 'gemma4',
    });
    expect(llamaServerStore.getSnapshot().status).not.toBeNull();
  });

  it('keeps polling after the only subscriber unsubscribes (unmount)', async () => {
    const listener = vi.fn();
    const unsubscribe = llamaServerStore.subscribe(listener);
    llamaServerStore.beginOperation('install', 'gemma4');

    await advanceTicks(2);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(2);

    unsubscribe();
    await advanceTicks(2);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(4);
    expect(llamaServerStore.getSnapshot().operation).not.toBeNull();
  });

  it('two subscribers share a single poll interval', async () => {
    const a = vi.fn();
    const b = vi.fn();
    llamaServerStore.subscribe(a);
    llamaServerStore.subscribe(b);
    llamaServerStore.beginOperation('install', 'gemma4');

    await advanceTicks(1);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(1);
    expect(a).toHaveBeenCalled();
    expect(b).toHaveBeenCalled();
  });

  it('a late subscriber immediately reads the latest snapshot', async () => {
    llamaServerStore.beginOperation('install', 'gemma4');
    mockLlamacppStatus.mockResolvedValue(
      statusResponse({ state: 'starting', detail: 'downloading 42%' })
    );
    await advanceTicks(1);

    // No prior subscription needed: getSnapshot reflects the poll results.
    const snap = llamaServerStore.getSnapshot();
    expect(snap.status?.sidecar.detail).toBe('downloading 42%');
    expect(snap.operation?.message).toBe('downloading 42%');
  });

  it('install operations stop polling once ready with the matching model', async () => {
    llamaServerStore.beginOperation('install', 'gemma4');
    const ready = llamaServerStore.waitForReady('gemma4');

    await advanceTicks(1);
    mockLlamacppStatus.mockResolvedValue(statusResponse({ state: 'ready', model: 'gemma4' }));
    await advanceTicks(1);

    await expect(ready).resolves.toBeUndefined();
    expect(llamaServerStore.getSnapshot().operation).toBeNull();

    const calls = mockLlamacppStatus.mock.calls.length;
    await advanceTicks(3);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(calls);
  });

  it('does not treat ready-with-other-model as terminal', async () => {
    llamaServerStore.beginOperation('install', 'gemma4-12b');
    mockLlamacppStatus.mockResolvedValue(statusResponse({ state: 'ready', model: 'gemma4' }));

    await advanceTicks(2);
    expect(llamaServerStore.getSnapshot().operation).not.toBeNull();
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(2);
  });

  it('start operations keep polling past ready until endOperation', async () => {
    llamaServerStore.beginOperation('start', 'gemma4');
    mockLlamacppStatus.mockResolvedValue(statusResponse({ state: 'ready', model: 'gemma4' }));

    await advanceTicks(2);
    expect(llamaServerStore.getSnapshot().operation).not.toBeNull();
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(2);

    llamaServerStore.endOperation();
    expect(llamaServerStore.getSnapshot().operation).toBeNull();
    await advanceTicks(2);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(2);
  });

  it('stops polling and rejects waiters on sidecar error', async () => {
    llamaServerStore.beginOperation('install', 'gemma4');
    const ready = llamaServerStore.waitForReady('gemma4');
    const rejection = expect(ready).rejects.toThrow('download failed: boom');

    mockLlamacppStatus.mockResolvedValue(
      statusResponse({ state: 'error', detail: 'download failed: boom' })
    );
    await advanceTicks(1);

    await rejection;
    expect(llamaServerStore.getSnapshot().operation).toBeNull();

    const calls = mockLlamacppStatus.mock.calls.length;
    await advanceTicks(2);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(calls);
  });

  it('times out after 60 minutes and stops polling', async () => {
    llamaServerStore.beginOperation('install', 'gemma4');
    const ready = llamaServerStore.waitForReady('gemma4');
    const rejection = expect(ready).rejects.toThrow('Timed out waiting for the local model');

    await vi.advanceTimersByTimeAsync(
      LLAMA_SERVER_OPERATION_TIMEOUT_MS + 2 * LLAMA_SERVER_POLL_INTERVAL_MS
    );

    await rejection;
    expect(llamaServerStore.getSnapshot().operation).toBeNull();

    const calls = mockLlamacppStatus.mock.calls.length;
    await advanceTicks(2);
    expect(mockLlamacppStatus).toHaveBeenCalledTimes(calls);
  });

  it('times out even when every status request fails', async () => {
    mockLlamacppStatus.mockRejectedValue(new Error('connection refused'));
    llamaServerStore.beginOperation('install', 'gemma4');
    const ready = llamaServerStore.waitForReady('gemma4');
    const rejection = expect(ready).rejects.toThrow('Timed out waiting for the local model');

    await vi.advanceTimersByTimeAsync(
      LLAMA_SERVER_OPERATION_TIMEOUT_MS + 2 * LLAMA_SERVER_POLL_INTERVAL_MS
    );

    await rejection;
    expect(llamaServerStore.getSnapshot().operation).toBeNull();
  });

  it('waitForReady rejects when no operation is polling', async () => {
    await expect(llamaServerStore.waitForReady('gemma4')).rejects.toThrow(
      'No Llama Server operation is polling'
    );
  });

  it('poll-less operations (Ollama pull) track messages and never poll', async () => {
    llamaServerStore.beginOperation('install', 'gemma4', 'Preparing install...', { poll: false });
    llamaServerStore.setOperationMessage('pulling manifest 10%');

    await advanceTicks(3);
    expect(mockLlamacppStatus).not.toHaveBeenCalled();
    expect(llamaServerStore.getSnapshot().operation?.message).toBe('pulling manifest 10%');

    llamaServerStore.endOperation();
    expect(llamaServerStore.getSnapshot().operation).toBeNull();
  });
});
