import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  STORAGE_KEY,
  loadDashboardState,
  saveDashboardState,
  filterDeadSessions,
  type SerializedDashboardState,
} from './dashboardStorage';

const makeState = (over: Partial<SerializedDashboardState> = {}): SerializedDashboardState => ({
  version: 1,
  windows: [],
  focusedWindowId: null,
  T1: 6,
  T2: 8,
  ...over,
});

beforeEach(() => {
  localStorage.clear();
});

describe('dashboardStorage', () => {
  it('returns null when nothing stored', () => {
    expect(loadDashboardState()).toBeNull();
  });

  it('round-trips state', () => {
    const state = makeState({ T1: 4, T2: 9 });
    saveDashboardState(state);
    expect(loadDashboardState()).toEqual(state);
  });

  it('returns null for malformed JSON', () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    expect(loadDashboardState()).toBeNull();
  });

  it('returns null when version mismatches', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...makeState(), version: 99 }));
    expect(loadDashboardState()).toBeNull();
  });

  it('filterDeadSessions removes windows whose sessionId is not present', async () => {
    const state = makeState({
      windows: [
        {
          windowId: 'w1',
          sessionId: 's1',
          name: 'A',
          badge: 1,
          accentColor: '#000',
          position: null,
          size: null,
          isManuallyPlaced: false,
          isTucked: false,
          lastInteraction: 0,
          unreadActivity: false,
        },
        {
          windowId: 'w2',
          sessionId: 's2',
          name: 'B',
          badge: 2,
          accentColor: '#111',
          position: null,
          size: null,
          isManuallyPlaced: false,
          isTucked: false,
          lastInteraction: 0,
          unreadActivity: false,
        },
      ],
      focusedWindowId: 'w2',
    });
    const isAlive = vi.fn(async (sid: string) => sid === 's1');
    const filtered = await filterDeadSessions(state, isAlive);
    expect(filtered.windows.map((w) => w.windowId)).toEqual(['w1']);
    expect(filtered.focusedWindowId).toBeNull();
  });

  it('migrates v1 key (biorouter.labmeeting.v1) to new key on load', () => {
    const v1State = {
      version: 1,
      windows: [],
      focusedWindowId: null,
      T1: 6,
      T2: 8,
    };
    localStorage.setItem('biorouter.labmeeting.v1', JSON.stringify(v1State));
    const loaded = loadDashboardState();
    expect(loaded).toEqual(v1State);
    expect(localStorage.getItem('biorouter.dashboard.v1')).not.toBeNull();
    expect(localStorage.getItem('biorouter.labmeeting.v1')).toBeNull();
  });

  it('does NOT overwrite existing new key if old key also present', () => {
    const oldState = { version: 1, windows: [], focusedWindowId: null, T1: 4, T2: 5 };
    const newState = { version: 1, windows: [], focusedWindowId: null, T1: 7, T2: 9 };
    localStorage.setItem('biorouter.labmeeting.v1', JSON.stringify(oldState));
    localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(newState));
    const loaded = loadDashboardState();
    expect(loaded?.T1).toBe(7);
    expect(loaded?.T2).toBe(9);
  });
});
