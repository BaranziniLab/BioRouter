import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Session } from '../api';
import { clearAllSessionCache, loadSession } from './sessionCache';

const cachedSession = {
  id: 'cache-proof',
  name: 'Cache proof',
  working_dir: '/tmp',
  conversation: [],
  created_at: '',
  updated_at: '',
  extension_data: {},
  message_count: 0,
  total_tokens: 0,
  user_set_name: false,
} as Session;

beforeEach(() => {
  clearAllSessionCache();
  vi.restoreAllMocks();
  Object.assign(window, {
    appConfig: { get: vi.fn(() => '') },
    electron: {
      getSecretKey: vi.fn(async () => 'daemon-secret'),
      getUserActionKey: vi.fn(async () => 'cache-user-proof'),
    },
  });
});

describe('sessionCache', () => {
  it('adds the user-action proof to its raw /agent/resume request', async () => {
    const fetchMock = vi.fn(async (..._args: Parameters<typeof globalThis.fetch>) => ({
      ok: true,
      json: async () => ({
        session: cachedSession,
        extension_results: null,
        initialization_error: null,
        active_turn: null,
      }),
    }));
    vi.stubGlobal('fetch', fetchMock);

    await expect(loadSession(cachedSession.id, true)).resolves.toEqual(cachedSession);

    expect(fetchMock).toHaveBeenCalledWith(
      '/agent/resume',
      expect.objectContaining({
        headers: expect.objectContaining({
          'Content-Type': 'application/json',
          'X-Secret-Key': 'daemon-secret',
          'X-User-Action': 'cache-user-proof',
        }),
      })
    );
    const request = fetchMock.mock.calls[0][1];
    expect(JSON.parse(request?.body as string)).toMatchObject({
      session_id: cachedSession.id,
      load_model_and_extensions: true,
      continuation_owner_id: expect.any(String),
    });
  });
});
