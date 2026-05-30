export const STORAGE_KEY = 'biorouter.dashboard.v2';
const STORAGE_KEY_V1 = 'biorouter.dashboard.v1';
const LEGACY_STORAGE_KEY = 'biorouter.labmeeting.v1';
const STORAGE_VERSION = 2;

// Defaults used when migrating windows that lacked an explicit position/size.
// Kept in sync with the constants in DashboardProvider/DashboardBoard.
const DEFAULT_W = 520;
const DEFAULT_H = 440;
const DEFAULT_GAP = 16;

export interface SerializedDashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;
  badge: number;
  accentColor: string;
  position: { x: number; y: number };
  size: { w: number; h: number };
  isManuallyPlaced: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
  /** Whether the window is currently rendered as a compact card. */
  folded?: boolean;
}

export interface SerializedDashboardState {
  version: number;
  windows: SerializedDashboardWindow[];
  focusedWindowId: string | null;
  cameraOffset: { x: number; y: number };
  /** Optional in storage so old payloads still load; provider defaults to false. */
  foldMode?: boolean;
}

// Legacy v1 shape, used only by the migration path.
interface LegacyV1Window {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName?: boolean;
  badge: number;
  accentColor: string;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  isManuallyPlaced: boolean;
  isTucked?: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
}

interface LegacyV1State {
  version: number;
  windows: LegacyV1Window[];
  focusedWindowId: string | null;
  T1?: number;
  T2?: number;
}

function migrateV1ToV2(v1: LegacyV1State): SerializedDashboardState {
  const windows: SerializedDashboardWindow[] = v1.windows.map((w, i) => {
    const position =
      w.position ?? { x: i * (DEFAULT_W + DEFAULT_GAP), y: 0 };
    const size = w.size ?? { w: DEFAULT_W, h: DEFAULT_H };
    // Drop isTucked; everything is on-canvas in v2.
    return {
      windowId: w.windowId,
      sessionId: w.sessionId,
      name: w.name,
      userSetName: typeof w.userSetName === 'boolean' ? w.userSetName : false,
      badge: w.badge,
      accentColor: w.accentColor,
      position,
      size,
      isManuallyPlaced: true,
      model: w.model,
      mode: w.mode,
      cwd: w.cwd,
      contextDepth: w.contextDepth,
      costAccumulated: w.costAccumulated,
      lastInteraction: w.lastInteraction,
      unreadActivity: w.unreadActivity,
      folded: false,
    };
  });
  return {
    version: STORAGE_VERSION,
    windows,
    focusedWindowId: v1.focusedWindowId,
    cameraOffset: { x: 0, y: 0 },
  };
}

export function loadDashboardState(): SerializedDashboardState | null {
  try {
    // v2: current shape, return directly.
    const rawV2 = localStorage.getItem(STORAGE_KEY);
    if (rawV2) {
      const parsed = JSON.parse(rawV2) as SerializedDashboardState;
      if (parsed.version === STORAGE_VERSION) return parsed;
    }
    // v1: migrate.
    const rawV1 = localStorage.getItem(STORAGE_KEY_V1);
    if (rawV1) {
      const parsedV1 = JSON.parse(rawV1) as LegacyV1State;
      const migrated = migrateV1ToV2(parsedV1);
      saveDashboardState(migrated);
      localStorage.removeItem(STORAGE_KEY_V1);
      return migrated;
    }
    // legacy labmeeting v1 → migrate via v1 path.
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (legacy) {
      const parsedLegacy = JSON.parse(legacy) as LegacyV1State;
      const migrated = migrateV1ToV2(parsedLegacy);
      saveDashboardState(migrated);
      localStorage.removeItem(LEGACY_STORAGE_KEY);
      return migrated;
    }
    return null;
  } catch {
    return null;
  }
}

export function saveDashboardState(state: SerializedDashboardState): void {
  try {
    const payload = { ...state, version: STORAGE_VERSION };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
  } catch {
    /* quota exceeded — drop silently */
  }
}

export function debounceSave(delayMs = 250): (state: SerializedDashboardState) => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (state) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => saveDashboardState(state), delayMs);
  };
}

export async function filterDeadSessions(
  state: SerializedDashboardState,
  isAlive: (sessionId: string) => Promise<boolean>
): Promise<SerializedDashboardState> {
  const checks = await Promise.all(state.windows.map((w) => isAlive(w.sessionId)));
  const aliveWindows = state.windows.filter((_, i) => checks[i]);
  const aliveIds = new Set(aliveWindows.map((w) => w.windowId));
  const focusedWindowId =
    state.focusedWindowId && aliveIds.has(state.focusedWindowId) ? state.focusedWindowId : null;
  return { ...state, windows: aliveWindows, focusedWindowId };
}
