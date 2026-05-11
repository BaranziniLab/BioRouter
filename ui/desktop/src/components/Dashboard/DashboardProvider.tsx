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
import { findSpawnPosition, organize as organizeLayout } from './canvasLayout';
import { createSession } from '../../sessions';
import { getInitialWorkingDir } from '../../utils/workingDir';

// Default window size — kept in sync with DashboardBoard.MIN_WINDOW_*.
const MIN_WINDOW_W = 520;
const MIN_WINDOW_H = 440;
const GAP = 16;

function nextWindowId(): string {
  return 'dw_' + Math.random().toString(36).slice(2, 10);
}

function serialize(state: DashboardState): SerializedDashboardState {
  return {
    version: 2,
    windows: state.windows.map((w) => ({ ...w })),
    focusedWindowId: state.focusedWindowId,
    cameraOffset: state.cameraOffset,
  };
}

function hydrate(): DashboardState {
  const raw = loadDashboardState();
  if (!raw) {
    return {
      windows: [],
      focusedWindowId: null,
      cameraOffset: { x: 0, y: 0 },
      organizeTick: 0,
      isHydrating: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({ ...w })),
    focusedWindowId: raw.focusedWindowId,
    cameraOffset: raw.cameraOffset ?? { x: 0, y: 0 },
    organizeTick: 0,
    isHydrating: false,
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
      const existing = prev.windows.map((w) => ({
        x: w.position.x,
        y: w.position.y,
        w: w.size.w,
        h: w.size.h,
      }));
      // Prefer placement adjacent to the focused window so new chats appear
      // right next to the active one. Without a focus, fall back to the
      // camera-center spiral.
      const focused = prev.focusedWindowId
        ? prev.windows.find((w) => w.windowId === prev.focusedWindowId)
        : null;
      const anchor = focused
        ? {
            x: focused.position.x,
            y: focused.position.y,
            w: focused.size.w,
            h: focused.size.h,
          }
        : null;
      const center = { x: -prev.cameraOffset.x, y: -prev.cameraOffset.y };
      const pos = findSpawnPosition({
        center,
        size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H },
        existing,
        gap: GAP,
        anchor,
      });
      const usedColors = prev.windows.map((w) => w.accentColor);
      const newWin: DashboardWindow = {
        windowId: nextWindowId(),
        sessionId,
        name: generateName(prev.windows.length),
        userSetName: false,
        badge: prev.windows.reduce((m, w) => Math.max(m, w.badge), 0) + 1,
        accentColor: pickAccentColor(usedColors),
        position: pos,
        size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H },
        isManuallyPlaced: true,
        cwd,
        lastInteraction: now,
        unreadActivity: false,
      };
      return {
        ...prev,
        windows: [...prev.windows, newWin],
        focusedWindowId: newWin.windowId,
      };
    });
  }, []);

  const closeWindow: DashboardApi['closeWindow'] = useCallback((windowId) => {
    setState((prev) => {
      const remaining = prev.windows.filter((w) => w.windowId !== windowId);
      let focusedWindowId = prev.focusedWindowId;
      if (focusedWindowId === windowId) {
        const candidates = [...remaining].sort((a, b) => b.lastInteraction - a.lastInteraction);
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
      windows: prev.windows.map((w) =>
        w.windowId === windowId ? { ...w, name, userSetName: true } : w
      ),
    }));
  }, []);

  const syncSessionName: DashboardApi['syncSessionName'] = useCallback((windowId, name) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId && !w.userSetName && w.name !== name ? { ...w, name } : w
      ),
    }));
  }, []);

  const moveWindow: DashboardApi['moveWindow'] = useCallback((windowId, position, size) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId
          ? {
              ...w,
              position,
              size: size ?? w.size,
              isManuallyPlaced: true,
              lastInteraction: Date.now(),
            }
          : w
      ),
    }));
  }, []);

  const freezeAllRects: DashboardApi['freezeAllRects'] = useCallback((rects) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => {
        const r = rects[w.windowId];
        if (!r) return w;
        if (w.isManuallyPlaced) return w;
        return {
          ...w,
          isManuallyPlaced: true,
          position: { x: r.x, y: r.y },
          size: { w: r.w, h: r.h },
        };
      }),
    }));
  }, []);

  const resizeWindow: DashboardApi['resizeWindow'] = useCallback((windowId, size, position) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId
          ? {
              ...w,
              size,
              position: position ?? w.position,
              isManuallyPlaced: true,
              lastInteraction: Date.now(),
            }
          : w
      ),
    }));
  }, []);

  const organize: DashboardApi['organize'] = useCallback(() => {
    setState((prev) => {
      if (prev.windows.length < 2) {
        return { ...prev, organizeTick: prev.organizeTick + 1 };
      }
      const anchor = prev.focusedWindowId ?? prev.windows[0].windowId;
      const rects = prev.windows.map((w) => ({
        id: w.windowId,
        x: w.position.x,
        y: w.position.y,
        w: w.size.w,
        h: w.size.h,
      }));
      const out = organizeLayout(rects, anchor, GAP);
      const byId = new Map(out.map((r) => [r.id, r]));
      return {
        ...prev,
        windows: prev.windows.map((w) => {
          const r = byId.get(w.windowId);
          return r ? { ...w, position: { x: r.x, y: r.y } } : w;
        }),
        organizeTick: prev.organizeTick + 1,
      };
    });
  }, []);

  const clearAll: DashboardApi['clearAll'] = useCallback(() => {
    setState((prev) => ({ ...prev, windows: [], focusedWindowId: null }));
  }, []);

  const panBy: DashboardApi['panBy'] = useCallback((dx, dy) => {
    setState((prev) => ({
      ...prev,
      cameraOffset: { x: prev.cameraOffset.x + dx, y: prev.cameraOffset.y + dy },
    }));
  }, []);

  const centerOn: DashboardApi['centerOn'] = useCallback((windowId, viewport) => {
    setState((prev) => {
      const w = prev.windows.find((x) => x.windowId === windowId);
      if (!w) return prev;
      const cx = w.position.x + w.size.w / 2;
      const cy = w.position.y + w.size.h / 2;
      return {
        ...prev,
        cameraOffset: {
          x: viewport.width / 2 - cx,
          y: viewport.height / 2 - cy,
        },
      };
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
      syncSessionName,
      moveWindow,
      resizeWindow,
      freezeAllRects,
      organize,
      clearAll,
      panBy,
      centerOn,
      updateWindowField,
      markActivity,
    }),
    [
      state,
      spawnWindow,
      closeWindow,
      focusWindow,
      renameWindow,
      syncSessionName,
      moveWindow,
      resizeWindow,
      freezeAllRects,
      organize,
      clearAll,
      panBy,
      centerOn,
      updateWindowField,
      markActivity,
    ]
  );

  return <DashboardContext.Provider value={api}>{children}</DashboardContext.Provider>;
};
