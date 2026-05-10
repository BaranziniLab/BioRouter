import { createContext, useContext } from 'react';

export interface DashboardWindow {
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

export interface DashboardState {
  windows: DashboardWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
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
  moveWindow: (windowId: string, position: { x: number; y: number }) => void;
  resizeWindow: (windowId: string, size: { w: number; h: number }) => void;
  tuckWindow: (windowId: string) => void;
  evokeWindow: (windowId: string, dropPos?: { x: number; y: number }) => void;
  organize: () => void;
  clearAll: () => void;
  setT1: (n: number) => void;
  setT2: (n: number) => void;
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
