import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  STORAGE_KEY,
  loadDashboardState,
  saveDashboardState,
  filterDeadSessions,
  type SerializedDashboardState,
  type SerializedDashboardWindow,
} from './dashboardStorage';

const makeWindow = (over: Partial<SerializedDashboardWindow> = {}): SerializedDashboardWindow => ({
  windowId: 'w1',
  sessionId: 's1',
  name: 'A',
  userSetName: false,
  badge: 1,
  accentColor: '#000',
  position: { x: 0, y: 0 },
  size: { w: 520, h: 440 },
  isManuallyPlaced: true,
  lastInteraction: 0,
  unreadActivity: false,
  ...over,
});

const makeState = (over: Partial<SerializedDashboardState> = {}): SerializedDashboardState => ({
  version: 2,
  windows: [],
  focusedWindowId: null,
  cameraOffset: { x: 0, y: 0 },
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
    const state = makeState({ cameraOffset: { x: 50, y: -30 } });
    saveDashboardState(state);
    expect(loadDashboardState()).toEqual(state);
  });

  it('returns null for malformed JSON', () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    expect(loadDashboardState()).toBeNull();
  });

  it('filterDeadSessions removes windows whose sessionId is not present', async () => {
    const state = makeState({
      windows: [
        makeWindow({ windowId: 'w1', sessionId: 's1' }),
        makeWindow({ windowId: 'w2', sessionId: 's2', badge: 2 }),
      ],
      focusedWindowId: 'w2',
    });
    const isAlive = vi.fn(async (sid: string) => sid === 's1');
    const filtered = await filterDeadSessions(state, isAlive);
    expect(filtered.windows.map((w) => w.windowId)).toEqual(['w1']);
    expect(filtered.focusedWindowId).toBeNull();
  });

  it('migrates v1 records by dropping isTucked and defaulting cameraOffset', () => {
    const v1 = {
      version: 1,
      windows: [
        {
          windowId: 'w1',
          sessionId: 's1',
          name: 'A',
          userSetName: false,
          badge: 1,
          accentColor: '#abcdef',
          position: { x: 10, y: 20 },
          size: { w: 520, h: 440 },
          isManuallyPlaced: true,
          isTucked: false,
          lastInteraction: 1,
          unreadActivity: false,
        },
      ],
      focusedWindowId: 'w1',
      T1: 6,
      T2: 8,
    };
    localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(v1));
    const loaded = loadDashboardState();
    expect(loaded).toBeTruthy();
    expect(loaded!.windows[0]).not.toHaveProperty('isTucked');
    expect(loaded!.cameraOffset).toEqual({ x: 0, y: 0 });
    expect(loaded!.version).toBe(2);
    // v1 key should be cleared after migration.
    expect(localStorage.getItem('biorouter.dashboard.v1')).toBeNull();
    // v2 should be written.
    expect(localStorage.getItem('biorouter.dashboard.v2')).not.toBeNull();
  });

  it('assigns position and size when v1 records lacked them', () => {
    const v1 = {
      version: 1,
      windows: [
        {
          windowId: 'tucked',
          sessionId: 's',
          name: 'T',
          userSetName: false,
          badge: 1,
          accentColor: '#abc',
          position: null,
          size: null,
          isManuallyPlaced: false,
          isTucked: true,
          lastInteraction: 1,
          unreadActivity: false,
        },
      ],
      focusedWindowId: null,
      T1: 6,
      T2: 8,
    };
    localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(v1));
    const loaded = loadDashboardState();
    expect(loaded!.windows.length).toBe(1);
    expect(loaded!.windows[0].position).toEqual({ x: 0, y: 0 });
    // Migration default = dashboardStorage.DEFAULT_{W,H} (760×440). Widened
    // from 600 so the streamlined composer keeps its controls inline.
    expect(loaded!.windows[0].size).toEqual({ w: 760, h: 440 });
  });

  it('migrates legacy labmeeting key into v2', () => {
    const legacy = {
      version: 1,
      windows: [],
      focusedWindowId: null,
      T1: 6,
      T2: 8,
    };
    localStorage.setItem('biorouter.labmeeting.v1', JSON.stringify(legacy));
    const loaded = loadDashboardState();
    expect(loaded).toBeTruthy();
    expect(loaded!.version).toBe(2);
    expect(localStorage.getItem('biorouter.labmeeting.v1')).toBeNull();
  });
});
