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
import { deleteSession } from '../../api';
import { getInitialWorkingDir } from '../../utils/workingDir';
import { toastError } from '../../toasts';
import { errorMessage } from '../../utils/conversionUtils';

// Default window size — kept in sync with DashboardBoard.MIN_WINDOW_*.
const MIN_WINDOW_W = 520;
const MIN_WINDOW_H = 440;
const GAP = 16;

// Folded-card geometry. Used by ChatWindow to render compact cards in place.
// Slightly wider + taller than the original 240×72 so the title (row 2) and
// the working-directory line (row 3) each get a full line without truncating
// or visually clipping at the leading.
export const CARD_W = 280;
export const CARD_H = 96;

function nextWindowId(): string {
  return 'dw_' + Math.random().toString(36).slice(2, 10);
}

function serialize(state: DashboardState): SerializedDashboardState {
  return {
    version: 2,
    windows: state.windows.map((w) => {
      // isBusy / previewTail are transient; never persisted.
      const { isBusy: _isBusy, previewTail: _previewTail, ...rest } = w;
      void _isBusy;
      void _previewTail;
      return { ...rest };
    }),
    focusedWindowId: state.focusedWindowId,
    cameraOffset: state.cameraOffset,
    foldMode: state.foldMode,
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
      foldMode: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({
      ...w,
      folded: w.folded ?? false,
      isBusy: false,
    })),
    focusedWindowId: raw.focusedWindowId,
    cameraOffset: raw.cameraOffset ?? { x: 0, y: 0 },
    organizeTick: 0,
    isAnimating: false,
    isHydrating: false,
    foldMode: raw.foldMode ?? false,
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

  const spawnWindow: DashboardApi['spawnWindow'] = useCallback(async (options) => {
    const generation = spawnGenerationRef.current;
    const cwd = options?.cwd?.trim() ? options.cwd : getInitialWorkingDir();
    // Resume-from-history skips createSession — the session already exists
    // and BaseChat will load it from its sessionId. The workflow path goes
    // through createSession so biorouterd attaches the workflow to the new
    // session up front, matching the new-Electron-window flow.
    let sessionId: string;
    if (options?.resumeSessionId) {
      sessionId = options.resumeSessionId;
    } else {
      // Surface backend failures (timeout, crashed agent holding LRU lock,
      // 5xx, etc.) instead of silently swallowing the rejected promise —
      // previously the user clicked Spawn and saw nothing happen.
      try {
        const session = await createSession(
          cwd,
          options?.workflowId ? { workflowId: options.workflowId } : undefined
        );
        if (generation !== spawnGenerationRef.current) return;
        sessionId = session.id;
      } catch (err) {
        if (generation !== spawnGenerationRef.current) return;
        toastError({
          title: 'Failed to spawn window',
          msg: errorMessage(err),
        });
        return;
      }
    }
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
      const overrideName = options?.name?.trim();
      const newWin: DashboardWindow = {
        windowId: nextWindowId(),
        sessionId,
        name: overrideName || generateName(prev.windows.length),
        // An override name is a "real" name from the workflow/history — mark
        // it user-set so the LLM auto-rename poller and backend default
        // placeholders don't overwrite it.
        userSetName: Boolean(overrideName),
        badge: prev.windows.reduce((m, w) => Math.max(m, w.badge), 0) + 1,
        accentColor: pickAccentColor(usedColors),
        position: pos,
        size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H },
        isManuallyPlaced: true,
        cwd,
        lastInteraction: now,
        unreadActivity: false,
        // In fold mode every new window starts folded — the user clicks the
        // card to expand it. Outside fold mode the old behavior holds.
        folded: prev.foldMode,
        isBusy: false,
      };
      return {
        ...prev,
        windows: [...prev.windows, newWin],
        focusedWindowId: newWin.windowId,
      };
    });
  }, []);

  const closeWindow: DashboardApi['closeWindow'] = useCallback((windowId) => {
    // Tell biorouterd to delete the session too, so the AgentManager LRU
    // doesn't accumulate zombie agents from closed windows — a crashed
    // zombie can hold its LRU write-lock and block future createSession
    // calls, manifesting as "Spawn does nothing" after Clear.
    const target = stateRef.current.windows.find((w) => w.windowId === windowId);
    if (target?.sessionId) {
      void deleteSession({
        path: { session_id: target.sessionId },
        throwOnError: false,
      }).catch(() => {
        // Best-effort: the window leaves the UI either way.
      });
    }
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
      // Folded windows are rendered at CARD_W × CARD_H regardless of their
      // stored size — pass those dimensions to the layout engine so it can
      // pack folded cards tightly instead of leaving full-window gaps.
      const rects = prev.windows.map((w) => ({
        id: w.windowId,
        x: w.position.x,
        y: w.position.y,
        w: w.folded ? CARD_W : w.size.w,
        h: w.folded ? CARD_H : w.size.h,
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
    // Tell biorouterd to delete each session too — without this, the
    // AgentManager LRU keeps holding the closed agents (including any
    // crashed ones whose background tasks still hold their LRU write-lock),
    // which can block subsequent createSession calls and make later Spawn
    // clicks appear to do nothing.
    const sessionIds = stateRef.current.windows.map((w) => w.sessionId);
    for (const sid of sessionIds) {
      void deleteSession({ path: { session_id: sid }, throwOnError: false }).catch(() => {
        // Best-effort cleanup.
      });
    }
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
      // Snap to integer device pixels. The world layer renders with
      // translate3d() + will-change: transform, so fractional offsets here
      // composite the GPU layer at sub-pixel positions and read as blurred
      // text on Retina displays.
      return {
        ...prev,
        cameraOffset: {
          x: Math.round(viewport.width / 2 - cx),
          y: Math.round(viewport.height / 2 - cy),
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

  const foldWindow: DashboardApi['foldWindow'] = useCallback((windowId, folded) => {
    flashAnimating();
    setState((prev) => {
      // In fold mode, unfolding a window auto-folds every other window — keeps
      // the canvas to a single visible chat at a time.
      const autoFoldSiblings = prev.foldMode && !folded;
      let mutated = false;
      const windows = prev.windows.map((w) => {
        if (w.windowId === windowId) {
          if (w.folded === folded) return w;
          mutated = true;
          return { ...w, folded };
        }
        if (autoFoldSiblings && !w.folded) {
          mutated = true;
          return { ...w, folded: true };
        }
        return w;
      });
      if (!mutated) return prev;
      return {
        ...prev,
        windows,
        // Make the just-unfolded window the focused one so centerOn brings it
        // into view. Folding doesn't change focus.
        focusedWindowId: !folded ? windowId : prev.focusedWindowId,
      };
    });
  }, [flashAnimating]);

  const foldAll: DashboardApi['foldAll'] = useCallback(() => {
    flashAnimating();
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => (w.folded ? w : { ...w, folded: true })),
    }));
  }, [flashAnimating]);

  const unfoldAll: DashboardApi['unfoldAll'] = useCallback(() => {
    flashAnimating();
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => (!w.folded ? w : { ...w, folded: false })),
    }));
  }, [flashAnimating]);

  const setFoldMode: DashboardApi['setFoldMode'] = useCallback((on) => {
    flashAnimating();
    setState((prev) => {
      if (prev.foldMode === on) return prev;
      // Entering fold mode folds everything; leaving unfolds everything.
      const windows = prev.windows.map((w) =>
        w.folded === on ? w : { ...w, folded: on }
      );
      return { ...prev, foldMode: on, windows };
    });
  }, [flashAnimating]);

  const setWindowPreview: DashboardApi['setWindowPreview'] = useCallback(
    (windowId, text) => {
      setState((prev) => {
        let mutated = false;
        const windows = prev.windows.map((w) => {
          if (w.windowId !== windowId) return w;
          const next = text ?? undefined;
          if (w.previewTail === next) return w;
          mutated = true;
          return { ...w, previewTail: next };
        });
        return mutated ? { ...prev, windows } : prev;
      });
    },
    []
  );

  const setWindowBusy: DashboardApi['setWindowBusy'] = useCallback((windowId, busy) => {
    setState((prev) => {
      let mutated = false;
      const windows = prev.windows.map((w) => {
        if (w.windowId !== windowId || w.isBusy === busy) return w;
        mutated = true;
        return { ...w, isBusy: busy };
      });
      return mutated ? { ...prev, windows } : prev;
    });
  }, []);

  const allFolded =
    state.windows.length > 0 && state.windows.every((w) => w.folded);

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
      foldWindow,
      foldAll,
      unfoldAll,
      setWindowBusy,
      setWindowPreview,
      setFoldMode,
      allFolded,
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
      foldWindow,
      foldAll,
      unfoldAll,
      setWindowBusy,
      setWindowPreview,
      setFoldMode,
      allFolded,
    ]
  );

  return <DashboardContext.Provider value={api}>{children}</DashboardContext.Provider>;
};
