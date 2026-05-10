import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  DashboardContext,
  DashboardApi,
  DashboardState,
  DashboardWindow,
} from '../../contexts/DashboardContext';
import { generateName, pickAccentColor } from './palette';
import {
  loadDashboardState,
  SerializedDashboardState,
  debounceSave,
} from './dashboardStorage';
import { createSession } from '../../sessions';
import { getInitialWorkingDir } from '../../utils/workingDir';

const DEFAULT_T1 = 6;
const DEFAULT_T2 = 8;

function nextWindowId(): string {
  return 'dw_' + Math.random().toString(36).slice(2, 10);
}

function serialize(state: DashboardState): SerializedDashboardState {
  return {
    version: 1,
    windows: state.windows.map((w) => ({ ...w })),
    focusedWindowId: state.focusedWindowId,
    T1: state.T1,
    T2: state.T2,
  };
}

function hydrate(): DashboardState {
  const raw = loadDashboardState();
  if (!raw) {
    return {
      windows: [],
      focusedWindowId: null,
      T1: DEFAULT_T1,
      T2: DEFAULT_T2,
      isHydrating: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({ ...w })),
    focusedWindowId: raw.focusedWindowId,
    T1: raw.T1,
    T2: raw.T2,
    isHydrating: false,
  };
}

function enforceT2Pure(s: DashboardState): DashboardState {
  const onBoard = s.windows.filter((w) => !w.isTucked);
  if (onBoard.length <= s.T2) return s;
  const focusedId = s.focusedWindowId;
  const sortedByOldest = [...onBoard]
    .filter((w) => w.windowId !== focusedId)
    .sort((a, b) => a.lastInteraction - b.lastInteraction);
  const numToTuck = onBoard.length - s.T2;
  const toTuckIds = new Set(sortedByOldest.slice(0, numToTuck).map((w) => w.windowId));
  return {
    ...s,
    windows: s.windows.map((w) =>
      toTuckIds.has(w.windowId) ? { ...w, isTucked: true } : w
    ),
  };
}

interface DashboardProviderProps {
  children: React.ReactNode;
}

export const DashboardProvider: React.FC<DashboardProviderProps> = ({ children }) => {
  const [state, setState] = useState<DashboardState>(() => hydrate());
  const debouncedSaveRef = useRef(debounceSave(250));

  useEffect(() => {
    debouncedSaveRef.current(serialize(state));
  }, [state]);

  const spawnWindow: DashboardApi['spawnWindow'] = useCallback(async () => {
    const cwd = getInitialWorkingDir();
    const session = await createSession(cwd);
    const sessionId = session.id;
    const now = Date.now();
    setState((prev) => {
      const usedColors = prev.windows.map((w) => w.accentColor);
      const accentColor = pickAccentColor(usedColors);
      const badge = prev.windows.reduce((m, w) => Math.max(m, w.badge), 0) + 1;
      const name = generateName(prev.windows.length);
      const newWin: DashboardWindow = {
        windowId: nextWindowId(),
        sessionId,
        name,
        badge,
        accentColor,
        position: null,
        size: null,
        isManuallyPlaced: false,
        isTucked: false,
        cwd,
        lastInteraction: now,
        unreadActivity: false,
      };
      const next: DashboardState = {
        ...prev,
        windows: [...prev.windows, newWin],
        focusedWindowId: newWin.windowId,
      };
      return enforceT2Pure(next);
    });
  }, []);

  const closeWindow: DashboardApi['closeWindow'] = useCallback((windowId) => {
    setState((prev) => {
      const remaining = prev.windows.filter((w) => w.windowId !== windowId);
      let focusedWindowId = prev.focusedWindowId;
      if (focusedWindowId === windowId) {
        const candidates = remaining
          .filter((w) => !w.isTucked)
          .sort((a, b) => b.lastInteraction - a.lastInteraction);
        focusedWindowId = candidates[0]?.windowId ?? null;
      }
      return { ...prev, windows: remaining, focusedWindowId };
    });
  }, []);

  const focusWindow: DashboardApi['focusWindow'] = useCallback((windowId) => {
    setState((prev) => ({
      ...prev,
      focusedWindowId: windowId,
      windows: prev.windows.map((w) =>
        w.windowId === windowId ? { ...w, lastInteraction: Date.now() } : w
      ),
    }));
  }, []);

  const renameWindow: DashboardApi['renameWindow'] = useCallback((windowId, name) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => (w.windowId === windowId ? { ...w, name } : w)),
    }));
  }, []);

  const moveWindow: DashboardApi['moveWindow'] = useCallback((windowId, position) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId
          ? { ...w, position, isManuallyPlaced: true, lastInteraction: Date.now() }
          : w
      ),
    }));
  }, []);

  const resizeWindow: DashboardApi['resizeWindow'] = useCallback((windowId, size) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId
          ? { ...w, size, isManuallyPlaced: true, lastInteraction: Date.now() }
          : w
      ),
    }));
  }, []);

  const tuckWindow: DashboardApi['tuckWindow'] = useCallback((windowId) => {
    setState((prev) => {
      const win = prev.windows.find((w) => w.windowId === windowId);
      if (!win || win.isTucked) return prev;
      const remainingOnBoard = prev.windows.filter(
        (w) => !w.isTucked && w.windowId !== windowId
      );
      let focusedWindowId = prev.focusedWindowId;
      if (focusedWindowId === windowId) {
        focusedWindowId =
          remainingOnBoard.sort((a, b) => b.lastInteraction - a.lastInteraction)[0]?.windowId ??
          null;
      }
      return {
        ...prev,
        windows: prev.windows.map((w) =>
          w.windowId === windowId
            ? { ...w, isTucked: true, isManuallyPlaced: false, position: null, size: null }
            : w
        ),
        focusedWindowId,
      };
    });
  }, []);

  const evokeWindow: DashboardApi['evokeWindow'] = useCallback((windowId, dropPos) => {
    setState((prev) => {
      const win = prev.windows.find((w) => w.windowId === windowId);
      if (!win) return prev;
      const next: DashboardState = {
        ...prev,
        windows: prev.windows.map((w) =>
          w.windowId === windowId
            ? {
                ...w,
                isTucked: false,
                position: dropPos ?? null,
                isManuallyPlaced: dropPos != null,
                unreadActivity: false,
                lastInteraction: Date.now(),
              }
            : w
        ),
        focusedWindowId: windowId,
      };
      return enforceT2Pure(next);
    });
  }, []);

  const organize: DashboardApi['organize'] = useCallback(() => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => ({
        ...w,
        isManuallyPlaced: false,
        position: null,
        size: null,
      })),
    }));
  }, []);

  const clearAll: DashboardApi['clearAll'] = useCallback(() => {
    setState((prev) => ({ ...prev, windows: [], focusedWindowId: null }));
  }, []);

  const setT1: DashboardApi['setT1'] = useCallback((n) => {
    setState((prev) => {
      const T1 = Math.max(1, Math.floor(n));
      return { ...prev, T1, T2: Math.max(prev.T2, T1) };
    });
  }, []);

  const setT2: DashboardApi['setT2'] = useCallback((n) => {
    setState((prev) => {
      const T2 = Math.max(prev.T1, Math.floor(n));
      return enforceT2Pure({ ...prev, T2 });
    });
  }, []);

  const updateWindowField: DashboardApi['updateWindowField'] = useCallback(
    (windowId, field, value) => {
      setState((prev) => ({
        ...prev,
        windows: prev.windows.map((w) =>
          w.windowId === windowId ? { ...w, [field]: value } : w
        ),
      }));
    },
    []
  );

  const markActivity: DashboardApi['markActivity'] = useCallback((windowId) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId && w.windowId !== prev.focusedWindowId
          ? { ...w, unreadActivity: true }
          : w
      ),
    }));
  }, []);

  const api: DashboardApi = useMemo(
    () => ({
      state,
      spawnWindow,
      closeWindow,
      focusWindow,
      renameWindow,
      moveWindow,
      resizeWindow,
      tuckWindow,
      evokeWindow,
      organize,
      clearAll,
      setT1,
      setT2,
      updateWindowField,
      markActivity,
    }),
    [
      state,
      spawnWindow,
      closeWindow,
      focusWindow,
      renameWindow,
      moveWindow,
      resizeWindow,
      tuckWindow,
      evokeWindow,
      organize,
      clearAll,
      setT1,
      setT2,
      updateWindowField,
      markActivity,
    ]
  );

  return <DashboardContext.Provider value={api}>{children}</DashboardContext.Provider>;
};
