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
  saveDashboardState,
  SerializedDashboardState,
  debounceSave,
} from './dashboardStorage';
import {
  isDefaultSessionName,
  subscribeSessionNameChanges,
} from '../../utils/sessionNameSync';
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
      isAnimating: false,
      isHydrating: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({ ...w })),
    focusedWindowId: raw.focusedWindowId,
    cameraOffset: raw.cameraOffset ?? { x: 0, y: 0 },
    organizeTick: 0,
    isAnimating: false,
    isHydrating: false,
  };
}

const ANIMATION_DURATION_MS = 220;

interface DashboardProviderProps {
  children: React.ReactNode;
}

export const DashboardProvider: React.FC<DashboardProviderProps> = ({ children }) => {
  const [state, setState] = useState<DashboardState>(() => hydrate());
  const debouncedSaveRef = useRef(debounceSave(250));
  const animationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Generation counter incremented on every clearAll. Each in-flight spawn
  // snapshots the current generation before awaiting createSession; if the
  // generation has advanced by the time the await resolves, the spawn aborts
  // instead of appending a phantom window to the just-cleared canvas.
  const spawnGenerationRef = useRef(0);

  useEffect(() => {
    debouncedSaveRef.current(serialize(state));
  }, [state]);

  // Keep a live ref to the current state so the unmount-flush below can
  // grab whatever was queued by the debounced save (which won't have fired
  // if the user navigates away within 250ms of the last edit).
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);
  useEffect(() => {
    return () => {
      // Synchronous flush — covers the route-change-faster-than-debounce
      // case that made renames "snap back" after navigating away and back.
      try {
        saveDashboardState(serialize(stateRef.current));
      } catch {
        // Ignore; the debounced save handles quota errors silently too.
      }
    };
  }, []);

  // Receive name changes from BaseChat's pill, dashboard sibling windows,
  // useChatStream's LLM-auto-rename poller, and any other open Electron
  // BrowserWindow. Drives the dashboard window title bar without polling.
  useEffect(() => {
    return subscribeSessionNameChanges((change) => {
      setState((prev) => {
        let mutated = false;
        const windows = prev.windows.map((w) => {
          if (w.sessionId !== change.sessionId) return w;
          if (change.userSetName) {
            if (w.name === change.name && w.userSetName) return w;
            mutated = true;
            return { ...w, name: change.name, userSetName: true };
          }
          // Non-user broadcast: don't overwrite a local user rename, and
          // don't accept a default placeholder over a real name.
          if (w.userSetName) return w;
          if (isDefaultSessionName(change.name)) return w;
          if (w.name === change.name) return w;
          mutated = true;
          return { ...w, name: change.name };
        });
        return mutated ? { ...prev, windows } : prev;
      });
    });
  }, []);

  // Briefly set isAnimating=true so windows + camera apply CSS transitions to
  // their next transform change. Cleared after ANIMATION_DURATION_MS.
  const flashAnimating = useCallback(() => {
    setState((prev) => (prev.isAnimating ? prev : { ...prev, isAnimating: true }));
    if (animationTimerRef.current) clearTimeout(animationTimerRef.current);
    animationTimerRef.current = setTimeout(() => {
      setState((prev) => (prev.isAnimating ? { ...prev, isAnimating: false } : prev));
    }, ANIMATION_DURATION_MS);
  }, []);

  useEffect(() => {
    return () => {
      if (animationTimerRef.current) clearTimeout(animationTimerRef.current);
    };
  }, []);

  const spawnWindow: DashboardApi['spawnWindow'] = useCallback(async () => {
    const generation = spawnGenerationRef.current;
    const cwd = getInitialWorkingDir();
    const session = await createSession(cwd);
    // If clearAll bumped the generation while we were awaiting, the user has
    // since hit Clear — drop the about-to-be-added window on the floor so the
    // empty canvas stays empty as the user expects.
    if (generation !== spawnGenerationRef.current) return;
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

  // `syncSessionName` is invoked from BaseChat's session-update effect
  // whenever the backend hands the renderer a name. The dashboard window
  // accepts the name when EITHER the backend marks it user-authored
  // (`userSetName`) OR it's a non-default LLM-generated name. We never let
  // a backend default ("New Session" / legacy numbered variants) overwrite
  // a non-default local name — that was the cause of the snap-back bug.
  const syncSessionName: DashboardApi['syncSessionName'] = useCallback(
    (windowId, name, opts) => {
      const isUserSet = opts?.userSetName ?? false;
      setState((prev) => ({
        ...prev,
        windows: prev.windows.map((w) => {
          if (w.windowId !== windowId) return w;
          // Always accept user-authored renames from the backend
          // (e.g. another window renamed the same session).
          if (isUserSet) {
            if (w.name === name && w.userSetName) return w;
            return { ...w, name, userSetName: true };
          }
          // Don't let backend's default placeholder overwrite an
          // LLM-rename we already accepted or a user rename here.
          if (w.userSetName) return w;
          if (isDefaultSessionName(name)) return w;
          if (w.name === name) return w;
          return { ...w, name };
        }),
      }));
    },
    []
  );

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

  // Note: includes flashAnimating in deps because we use it inside.
  const organize: DashboardApi['organize'] = useCallback(() => {
    flashAnimating();
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
  }, [flashAnimating]);

  const clearAll: DashboardApi['clearAll'] = useCallback(() => {
    // Invalidate any in-flight spawns (createSession promises that haven't
    // resolved yet) so they don't append a phantom window after the user
    // explicitly cleared the canvas.
    spawnGenerationRef.current += 1;
    setState((prev) => ({ ...prev, windows: [], focusedWindowId: null }));
  }, []);

  const panBy: DashboardApi['panBy'] = useCallback((dx, dy) => {
    setState((prev) => {
      // Ignore pan/wheel deltas while a centering animation is in flight,
      // otherwise a user dragging mid-spawn (or mid-organize) would have
      // their pan delta accumulate on top of the just-set cameraOffset,
      // leaving the camera offset from the intended center by however far
      // they happened to drag during the 220ms animation window.
      if (prev.isAnimating) return prev;
      return {
        ...prev,
        cameraOffset: { x: prev.cameraOffset.x + dx, y: prev.cameraOffset.y + dy },
      };
    });
  }, []);

  const centerOn: DashboardApi['centerOn'] = useCallback((windowId, viewport) => {
    flashAnimating();
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
  }, [flashAnimating]);

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
