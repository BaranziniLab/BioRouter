import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ActivityWindow, Session } from '../api';

const mocks = vi.hoisted(() => ({
  getSessionActivity: vi.fn(),
}));

vi.mock('../api', () => ({
  getSessionActivity: mocks.getSessionActivity,
}));

const activity: ActivityWindow = {
  start: '2026-02-11',
  end: '2026-07-15',
  maxSessions: 4,
  maxTokens: 1000,
  tokensComplete: true,
  currentStreak: 3,
  longestStreak: 8,
  days: [],
};

const recentSession: Session = {
  id: 'session-1',
  name: 'Persisted session',
  created_at: '2026-07-14T12:00:00Z',
  updated_at: '2026-07-15T12:00:00Z',
  extension_data: {},
  message_count: 2,
  working_dir: '/Users/test',
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.resetModules();
  localStorage.clear();
});

describe('homeInsightsCache', () => {
  it('restores activity and recent chats after a module reload', async () => {
    const cache = await import('./homeInsightsCache');
    cache.cacheHomeActivity(activity);
    cache.cacheHomeRecentSessions([recentSession]);

    vi.resetModules();
    const restoredCache = await import('./homeInsightsCache');

    expect(restoredCache.getCachedHomeActivity()).toEqual(activity);
    expect(restoredCache.getCachedRecentSessions()).toEqual([recentSession]);
  });

  it('deduplicates overlapping activity prefetch and view requests', async () => {
    let finishRequest: ((value: { data: ActivityWindow }) => void) | undefined;
    mocks.getSessionActivity.mockReturnValue(
      new Promise((resolve) => {
        finishRequest = resolve;
      })
    );
    const cache = await import('./homeInsightsCache');

    cache.preloadHomeActivity();
    const viewLoad = cache.refreshHomeActivity();

    expect(mocks.getSessionActivity).toHaveBeenCalledTimes(1);
    finishRequest?.({ data: activity });
    await viewLoad;
    expect(cache.getCachedHomeActivity()).toEqual(activity);
  });

  it('does not prefetch again when persisted activity is available', async () => {
    const cache = await import('./homeInsightsCache');
    cache.cacheHomeActivity(activity);

    cache.preloadHomeActivity();

    expect(mocks.getSessionActivity).not.toHaveBeenCalled();
  });
});
