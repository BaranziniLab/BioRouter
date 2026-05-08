export const STORAGE_KEY = 'biorouter.labmeeting.v1';
const STORAGE_VERSION = 1;

export interface SerializedLabWindow {
  windowId: string;
  sessionId: string;
  name: string;
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

export interface SerializedLabMeetingState {
  version: number;
  windows: SerializedLabWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
}

export function loadLabMeetingState(): SerializedLabMeetingState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SerializedLabMeetingState;
    if (parsed.version !== STORAGE_VERSION) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function saveLabMeetingState(state: SerializedLabMeetingState): void {
  try {
    const payload = { ...state, version: STORAGE_VERSION };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
  } catch {
    /* quota exceeded — drop silently */
  }
}

export function debounceSave(delayMs = 250): (state: SerializedLabMeetingState) => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (state) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => saveLabMeetingState(state), delayMs);
  };
}

export async function filterDeadSessions(
  state: SerializedLabMeetingState,
  isAlive: (sessionId: string) => Promise<boolean>
): Promise<SerializedLabMeetingState> {
  const checks = await Promise.all(state.windows.map((w) => isAlive(w.sessionId)));
  const aliveWindows = state.windows.filter((_, i) => checks[i]);
  const aliveIds = new Set(aliveWindows.map((w) => w.windowId));
  const focusedWindowId =
    state.focusedWindowId && aliveIds.has(state.focusedWindowId) ? state.focusedWindowId : null;
  return { ...state, windows: aliveWindows, focusedWindowId };
}
