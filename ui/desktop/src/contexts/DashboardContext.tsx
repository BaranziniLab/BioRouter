import { createContext, useContext } from 'react';

export interface DashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;
  badge: number;
  accentColor: string;
  /** World-space coordinates. Set at spawn; never null. */
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
}

export interface DashboardState {
  windows: DashboardWindow[];
  focusedWindowId: string | null;
  /** Viewport→world translation. The world layer renders with
   * `translate(cameraOffset.x, cameraOffset.y)`. Panning increments these. */
  cameraOffset: { x: number; y: number };
  /** Bumped each time `organize` runs so consumers can re-center the camera. */
  organizeTick: number;
  /** True for ~220ms after an organize / centerOn so consumers can apply a
   * CSS transition to window + camera transforms. Cleared automatically. */
  isAnimating: boolean;
  isHydrating: boolean;
}

export interface DashboardApi {
  state: DashboardState;
  spawnWindow: () => Promise<void>;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  renameWindow: (windowId: string, name: string) => void;
  /** Called when biorouterd auto-names the session. Updates only if userSetName is false. */
  syncSessionName: (windowId: string, name: string) => void;
  moveWindow: (
    windowId: string,
    position: { x: number; y: number },
    /** Optional: preserve current size so the layout doesn't fall back to defaults. */
    size?: { w: number; h: number }
  ) => void;
  resizeWindow: (
    windowId: string,
    size: { w: number; h: number },
    /** Optional: preserve current position so resize doesn't reset it. */
    position?: { x: number; y: number }
  ) => void;
  /** Pin every on-canvas window at the given rects with isManuallyPlaced=true.
   * Called by the board at drag/resize start so manipulating one window never
   * triggers an automatic re-layout of the others. */
  freezeAllRects: (
    rects: Record<string, { x: number; y: number; w: number; h: number }>
  ) => void;
  organize: () => void;
  clearAll: () => void;
  /** Pan the camera by (dx, dy) in viewport pixels. */
  panBy: (dx: number, dy: number) => void;
  /** Recenter camera so the given window's center maps to the viewport center. */
  centerOn: (windowId: string, viewport: { width: number; height: number }) => void;
  updateWindowField: <K extends keyof DashboardWindow>(
    windowId: string,
    field: K,
    value: DashboardWindow[K]
  ) => void;
  markActivity: (windowId: string) => void;
}

export const DashboardContext = createContext<DashboardApi | null>(null);

export const useDashboard = (): DashboardApi => {
  const ctx = useContext(DashboardContext);
  if (!ctx) throw new Error('useDashboard must be used inside DashboardProvider');
  return ctx;
};

export const useOptionalDashboard = (): DashboardApi | null => useContext(DashboardContext);
