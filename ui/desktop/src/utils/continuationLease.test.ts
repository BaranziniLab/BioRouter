import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getContinuationOwnerId, recoverContinuationGroup } from './continuationLease';

beforeEach(() => {
  sessionStorage.clear();
  vi.restoreAllMocks();
  Object.assign(window, {
    appConfig: { get: vi.fn(() => '') },
    electron: {
      getSecretKey: vi.fn(async () => 'daemon-secret'),
      getUserActionKey: vi.fn(async () => 'user-action-proof'),
    },
  });
});

describe('continuation lease recovery', () => {
  it('keeps one window owner stable in session storage', () => {
    const first = getContinuationOwnerId();
    const second = getContinuationOwnerId();

    expect(first).toBeTruthy();
    expect(second).toBe(first);
    expect(sessionStorage.getItem('biorouter.continuation-owner.v1')).toBe(first);
  });

  it('sends proof, exact generation, and the stable owner for explicit takeover', async () => {
    const fetchMock = vi.fn(async (..._args: Parameters<typeof fetch>) => ({
      ok: true,
      json: async () => ({
        resolution: 'taken_over',
        superseded_turn_id: 'turn-stopped',
        continuation_lease: 'lease-recovered',
      }),
    }));
    vi.stubGlobal('fetch', fetchMock);
    const ownerId = getContinuationOwnerId();

    await expect(
      recoverContinuationGroup('child-session', 'turn-stopped', 'take_over')
    ).resolves.toMatchObject({ continuation_lease: 'lease-recovered' });

    const request = fetchMock.mock.calls[0][1] as RequestInit;
    expect(request.headers).toMatchObject({
      'X-Secret-Key': 'daemon-secret',
      'X-User-Action': 'user-action-proof',
    });
    expect(JSON.parse(request.body as string)).toEqual({
      session_id: 'child-session',
      superseded_turn_id: 'turn-stopped',
      continuation_owner_id: ownerId,
      action: 'take_over',
    });
  });
});
