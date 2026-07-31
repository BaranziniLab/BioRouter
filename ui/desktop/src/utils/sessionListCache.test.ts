import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearSessionListCache,
  getCachedSessionList,
  notifySessionListChanged,
  preloadSessionList,
  refreshSessionList,
  subscribeSessionListChanges,
} from './sessionListCache';
import { announceSessionName } from './sessionNameSync';

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
  updateSessionName: vi.fn(),
}));

vi.mock('../api', () => ({
  listSessions: mocks.listSessions,
  updateSessionName: mocks.updateSessionName,
}));

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
});

describe('sessionListCache', () => {
  it('deduplicates a prefetch and a view load that overlap', async () => {
    let finishRequest: ((value: { data: { sessions: never[] } }) => void) | undefined;
    mocks.listSessions.mockReturnValue(
      new Promise((resolve) => {
        finishRequest = resolve;
      })
    );

    preloadSessionList();
    const viewLoad = refreshSessionList();

    expect(mocks.listSessions).toHaveBeenCalledTimes(1);
    finishRequest?.({ data: { sessions: [] } });
    await viewLoad;
    expect(getCachedSessionList()).toEqual([]);
  });

  it('does not prefetch again after the list is cached', async () => {
    mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });

    await refreshSessionList();
    preloadSessionList();

    expect(mocks.listSessions).toHaveBeenCalledTimes(1);
  });

  it('patches a cached session name when a rename is announced', async () => {
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [{ id: 's1', name: 'Old name', user_set_name: false, working_dir: '/x' }] },
    });
    await refreshSessionList();

    // A rename rides the name channel; the See-all / Home cache must reflect it
    // so those surfaces do not show an old name beside the tab's new one.
    announceSessionName({ sessionId: 's1', name: 'New name', userSetName: true, origin: 'user' });

    expect(getCachedSessionList()?.[0]).toMatchObject({ name: 'New name', user_set_name: true });
  });

  it('a keyless refresh keeps the flagged identity instead of clobbering it', async () => {
    mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });
    await refreshSessionList(true);
    expect(mocks.listSessions).toHaveBeenLastCalledWith(
      expect.objectContaining({ query: { include_subagents: true } })
    );
    mocks.listSessions.mockClear();
    // Home's loader. It has no opinion, so it must not invalidate History's:
    // whatever it re-reads, it re-reads with the identity History asked for.
    // It re-reads (this function has no cache-hit short-circuit — a membership
    // change depends on that), but it must re-read the flagged identity.
    await refreshSessionList();
    expect(mocks.listSessions).toHaveBeenLastCalledWith(
      expect.objectContaining({ query: { include_subagents: true } })
    );
  });

  it('notifies list-change subscribers and re-reads on a membership change', async () => {
    mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });
    await refreshSessionList();
    mocks.listSessions.mockClear();

    const listener = vi.fn();
    const unsub = subscribeSessionListChanges(listener);

    // A diverge / create / delete announces membership; every list surface
    // re-reads without waiting for an unrelated turn.
    notifySessionListChanged();

    expect(listener).toHaveBeenCalledTimes(1);
    expect(mocks.listSessions).toHaveBeenCalled();
    unsub();
  });
});
