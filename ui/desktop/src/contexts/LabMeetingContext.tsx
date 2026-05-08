import { createContext, useContext } from 'react';

export interface LabWindow {
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

export interface LabMeetingState {
  windows: LabWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
  isHydrating: boolean;
}

export interface LabMeetingApi {
  state: LabMeetingState;
  spawnWindow: () => Promise<void>;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  renameWindow: (windowId: string, name: string) => void;
  moveWindow: (windowId: string, position: { x: number; y: number }) => void;
  resizeWindow: (windowId: string, size: { w: number; h: number }) => void;
  tuckWindow: (windowId: string) => void;
  evokeWindow: (windowId: string, dropPos?: { x: number; y: number }) => void;
  organize: () => void;
  clearAll: () => void;
  setT1: (n: number) => void;
  setT2: (n: number) => void;
  updateWindowField: <K extends keyof LabWindow>(
    windowId: string,
    field: K,
    value: LabWindow[K]
  ) => void;
  markActivity: (windowId: string) => void;
}

export const LabMeetingContext = createContext<LabMeetingApi | null>(null);

export const useLabMeeting = (): LabMeetingApi => {
  const ctx = useContext(LabMeetingContext);
  if (!ctx) throw new Error('useLabMeeting must be used inside LabMeetingProvider');
  return ctx;
};

export const useOptionalLabMeeting = (): LabMeetingApi | null => useContext(LabMeetingContext);
