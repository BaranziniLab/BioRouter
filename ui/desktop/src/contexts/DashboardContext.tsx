import { createContext, useContext } from 'react';

export interface DashboardWindow {
  windowId: string;
  sessionId: string;
  /** True when this window's spawn CREATED the session (a fresh "New chat"),
   * false when it resumed/diverged an already-existing session. Closing only
   * ever deletes a created-here session that is still empty; resumed and
   * diverged sessions are always preserved in history. */
  createdHere?: boolean;
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
  /** When true, render as a compact FoldedCard instead of the full chat. */
  folded: boolean;
  /** Transient: true while the inner chat is streaming or running a tool. Not persisted. */
  isBusy: boolean;
  /** Transient: short tail of the last assistant message text (or current streaming
   * fragment). Shown in the hover preview over the folded card. Not persisted. */
  previewTail?: string;
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
  /** Global "fold mode": when true, new windows spawn folded, at most one
   * card is unfolded at a time, and clicking the canvas refolds whatever
   * window is currently expanded. Persists across reloads. */
  foldMode: boolean;
}

export interface SpawnWindowOptions {
  /** Resume an existing session instead of creating a new one. The dashboard
   * window will display the session's history; no new session is created. */
  resumeSessionId?: string;
  /** Create the new session with a workflow attached. Ignored when
   * `resumeSessionId` is set. */
  workflowId?: string;
  /** Working directory for the new session. Defaults to the app's initial
   * working dir. When resuming, this is informational only. */
  cwd?: string;
  /** Override the auto-generated window display name (e.g. workflow title or
   * resumed-session name). */
  name?: string;
}

export interface DashboardApi {
  state: DashboardState;
  spawnWindow: (options?: SpawnWindowOptions) => Promise<void>;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  renameWindow: (windowId: string, name: string) => void;
  /** Apply a session name observed from the backend (LLM auto-rename or a
   * sibling window's rename). Pass `opts.userSetName: true` when the
   * backend marks it user-authored — the window then accepts and locks it
   * even if it matches a default placeholder.
   *
   * Without `opts.userSetName`, the window only accepts the name if it
   * isn't a default placeholder and the window hasn't been renamed
   * locally (`userSetName=false` on the window). */
  syncSessionName: (windowId: string, name: string, opts?: { userSetName?: boolean }) => void;
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
  freezeAllRects: (rects: Record<string, { x: number; y: number; w: number; h: number }>) => void;
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
  /** Toggle a single window between folded and unfolded. */
  foldWindow: (windowId: string, folded: boolean) => void;
  /** Set every on-canvas window's `folded` to true. */
  foldAll: () => void;
  /** Set every on-canvas window's `folded` to false. */
  unfoldAll: () => void;
  /** Mark a window's chat as actively streaming/running. Driven by BaseChat. */
  setWindowBusy: (windowId: string, busy: boolean) => void;
  /** Update the transient preview text shown in a folded card's hover popup. */
  setWindowPreview: (windowId: string, text: string | null) => void;
  /** Enter or leave global fold mode. Entering folds every window; leaving
   * unfolds every window. */
  setFoldMode: (on: boolean) => void;
  /** Derived: every on-canvas window has folded=true. False when windows is empty. */
  allFolded: boolean;
}

export const DashboardContext = createContext<DashboardApi | null>(null);

export const useDashboard = (): DashboardApi => {
  const ctx = useContext(DashboardContext);
  if (!ctx) throw new Error('useDashboard must be used inside DashboardProvider');
  return ctx;
};

export const useOptionalDashboard = (): DashboardApi | null => useContext(DashboardContext);
