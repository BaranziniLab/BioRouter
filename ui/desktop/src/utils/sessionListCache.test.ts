import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearSessionListCache,
  getCachedSessionList,
  preloadSessionList,
  refreshSessionList,
} from './sessionListCache';

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
}));

vi.mock('../api', () => ({
  listSessions: mocks.listSessions,
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
});
