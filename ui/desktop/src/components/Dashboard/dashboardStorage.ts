export const STORAGE_KEY = 'biorouter.dashboard.v1';
const LEGACY_STORAGE_KEY = 'biorouter.labmeeting.v1';
const STORAGE_VERSION = 1;

export interface SerializedDashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;
  badge: number;
  accentColor: string;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  isManuallyPlaced: boolean;
  isTucked: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
}

export interface SerializedDashboardState {
  version: number;
  windows: SerializedDashboardWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
}

export function loadDashboardState(): SerializedDashboardState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as SerializedDashboardState;
      if (parsed.version === STORAGE_VERSION) {
        parsed.windows = parsed.windows.map((w) =>
          typeof w.userSetName === 'boolean' ? w : { ...w, userSetName: false }
        );
        return parsed;
      }
      return null;
    }
    // Migration: try the legacy v1 key (biorouter.labmeeting.v1)
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!legacy) return null;
    const parsedLegacy = JSON.parse(legacy) as SerializedDashboardState;
    if (parsedLegacy.version !== STORAGE_VERSION) return null;
    parsedLegacy.windows = parsedLegacy.windows.map((w) =>
      typeof w.userSetName === 'boolean' ? w : { ...w, userSetName: false }
    );
    saveDashboardState(parsedLegacy);
    localStorage.removeItem(LEGACY_STORAGE_KEY);
    return parsedLegacy;
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
