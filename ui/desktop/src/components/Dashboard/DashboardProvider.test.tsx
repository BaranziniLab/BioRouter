import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { DashboardProvider } from './DashboardProvider';
import { useDashboard } from '../../contexts/DashboardContext';

vi.mock('../../sessions', () => ({
  createSession: vi.fn(async () => ({ id: 'sess_' + Math.random().toString(36).slice(2, 6) })),
}));
vi.mock('../../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/tmp',
}));
// The provider now reaps agents/sessions on close via these — resolve them
// cleanly so the fire-and-forget calls don't leak rejections in jsdom.
vi.mock('../../api', () => ({
  deleteSession: vi.fn(async () => ({ data: {}, error: undefined })),
  getSession: vi.fn(async () => ({ data: { message_count: 1 }, error: undefined })),
  stopAgent: vi.fn(async () => ({ data: {}, error: undefined })),
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <DashboardProvider>{children}</DashboardProvider>
);

beforeEach(() => {
  localStorage.clear();
});

describe('DashboardProvider (canvas mode)', () => {
  it('starts empty with cameraOffset at origin', () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    expect(result.current.state.windows).toHaveLength(0);
    expect(result.current.state.cameraOffset).toEqual({ x: 0, y: 0 });
    expect(result.current.state.organizeTick).toBe(0);
  });

  it('spawn adds a window with concrete position/size and focuses it', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    expect(result.current.state.windows).toHaveLength(1);
    const w = result.current.state.windows[0];
    expect(result.current.state.focusedWindowId).toBe(w.windowId);
    expect(w.position).toBeDefined();
    // Spawn default size = DashboardProvider.MIN_WINDOW_{W,H}.
    expect(w.size).toEqual({ w: 600, h: 440 });
  });

  it('spawning multiple windows places them non-overlappingly', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    for (let i = 0; i < 5; i++) {
      await act(async () => {
        await result.current.spawnWindow();
      });
    }
    const windows = result.current.state.windows;
    expect(windows).toHaveLength(5);
    // No two windows should overlap.
    for (let i = 0; i < windows.length; i++) {
      for (let j = i + 1; j < windows.length; j++) {
        const a = windows[i];
        const b = windows[j];
        const overlapW = Math.max(
          0,
          Math.min(a.position.x + a.size.w, b.position.x + b.size.w) -
            Math.max(a.position.x, b.position.x)
        );
        const overlapH = Math.max(
          0,
          Math.min(a.position.y + a.size.h, b.position.y + b.size.h) -
            Math.max(a.position.y, b.position.y)
        );
        expect(overlapW === 0 || overlapH === 0).toBe(true);
      }
    }
  });

  it('closeWindow drops the window and re-focuses most recent', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const [w1, w2] = result.current.state.windows;
    act(() => result.current.closeWindow(w2.windowId));
    expect(result.current.state.windows).toHaveLength(1);
    expect(result.current.state.focusedWindowId).toBe(w1.windowId);
  });

  it('renameWindow persists name', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.renameWindow(id, 'Mass Spec Run'));
    expect(result.current.state.windows[0].name).toBe('Mass Spec Run');
  });

  it('organize preserves sizes, resolves overlaps, bumps tick', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const [w1, w2] = result.current.state.windows;
    // Force them to overlap.
    act(() => result.current.moveWindow(w1.windowId, { x: 0, y: 0 }, { w: 520, h: 440 }));
    act(() => result.current.moveWindow(w2.windowId, { x: 100, y: 100 }, { w: 520, h: 440 }));
    const tickBefore = result.current.state.organizeTick;
    act(() => result.current.organize());
    expect(result.current.state.organizeTick).toBe(tickBefore + 1);
    // Sizes preserved.
    expect(result.current.state.windows[0].size).toEqual({ w: 520, h: 440 });
    expect(result.current.state.windows[1].size).toEqual({ w: 520, h: 440 });
    // No overlap.
    const a = result.current.state.windows[0];
    const b = result.current.state.windows[1];
    const overlapW = Math.max(
      0,
      Math.min(a.position.x + a.size.w, b.position.x + b.size.w) -
        Math.max(a.position.x, b.position.x)
    );
    const overlapH = Math.max(
      0,
      Math.min(a.position.y + a.size.h, b.position.y + b.size.h) -
        Math.max(a.position.y, b.position.y)
    );
    expect(overlapW === 0 || overlapH === 0).toBe(true);
  });

  it('panBy increments cameraOffset', () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    act(() => result.current.panBy(10, -20));
    expect(result.current.state.cameraOffset).toEqual({ x: 10, y: -20 });
    act(() => result.current.panBy(5, 5));
    expect(result.current.state.cameraOffset).toEqual({ x: 15, y: -15 });
  });

  it('centerOn places the window center at viewport center', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    // Move the window to a known position.
    act(() => result.current.moveWindow(id, { x: 1000, y: 500 }, { w: 200, h: 100 }));
    act(() => result.current.centerOn(id, { width: 800, height: 600 }));
    // World center at (1100, 550). Viewport center at (400, 300). Offset = (400 - 1100, 300 - 550) = (-700, -250).
    expect(result.current.state.cameraOffset).toEqual({ x: -700, y: -250 });
  });

  it('clearAll removes all windows', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    act(() => result.current.clearAll());
    expect(result.current.state.windows).toHaveLength(0);
  });

  it('syncSessionName updates name when userSetName is false', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.syncSessionName(id, 'Auto-named by AI'));
    expect(result.current.state.windows[0].name).toBe('Auto-named by AI');
    expect(result.current.state.windows[0].userSetName).toBe(false);
  });

  it('syncSessionName respects userSetName and does NOT overwrite user rename', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.renameWindow(id, 'My Project'));
    expect(result.current.state.windows[0].userSetName).toBe(true);
    act(() => result.current.syncSessionName(id, 'Auto-named by AI'));
    expect(result.current.state.windows[0].name).toBe('My Project');
  });

  describe('fold mode', () => {
    it('foldWindow flips a single window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      expect(result.current.state.windows[0].folded).toBe(false);
      act(() => result.current.foldWindow(id, true));
      expect(result.current.state.windows[0].folded).toBe(true);
      act(() => result.current.foldWindow(id, false));
      expect(result.current.state.windows[0].folded).toBe(false);
    });

    it('foldAll folds every window; unfoldAll unfolds every window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      act(() => result.current.foldAll());
      expect(result.current.state.windows.every((w) => w.folded)).toBe(true);
      act(() => result.current.unfoldAll());
      expect(result.current.state.windows.every((w) => !w.folded)).toBe(true);
    });

    it('allFolded reflects derived state', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      expect(result.current.allFolded).toBe(false); // empty
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      expect(result.current.allFolded).toBe(false);
      act(() => result.current.foldAll());
      expect(result.current.allFolded).toBe(true);
      const firstId = result.current.state.windows[0].windowId;
      act(() => result.current.foldWindow(firstId, false));
      expect(result.current.allFolded).toBe(false);
    });

    it('setWindowBusy updates only the matching window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      const [a, b] = result.current.state.windows.map((w) => w.windowId);
      act(() => result.current.setWindowBusy(a, true));
      const winA = result.current.state.windows.find((w) => w.windowId === a)!;
      const winB = result.current.state.windows.find((w) => w.windowId === b)!;
      expect(winA.isBusy).toBe(true);
      expect(winB.isBusy).toBe(false);
    });

    it('isBusy is not persisted across hydrate', async () => {
      const { result, unmount } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      act(() => result.current.setWindowBusy(id, true));
      expect(result.current.state.windows[0].isBusy).toBe(true);
      unmount(); // flushes save synchronously via the unmount-cleanup effect
      const { result: result2 } = renderHook(() => useDashboard(), { wrapper });
      expect(result2.current.state.windows[0].isBusy).toBe(false);
    });

    it('folded is persisted across hydrate', async () => {
      const { result, unmount } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      act(() => result.current.foldWindow(id, true));
      unmount();
      const { result: result2 } = renderHook(() => useDashboard(), { wrapper });
      expect(result2.current.state.windows[0].folded).toBe(true);
    });
  });
});
