import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { LabMeetingProvider } from './LabMeetingProvider';
import { useLabMeeting } from '../../contexts/LabMeetingContext';

vi.mock('../../sessions', () => ({
  createSession: vi.fn(async () => ({ id: 'sess_' + Math.random().toString(36).slice(2, 6) })),
}));
vi.mock('../../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/tmp',
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <LabMeetingProvider>{children}</LabMeetingProvider>
);

beforeEach(() => {
  localStorage.clear();
});

describe('LabMeetingProvider', () => {
  it('starts empty', () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    expect(result.current.state.windows).toHaveLength(0);
    expect(result.current.state.T1).toBe(6);
    expect(result.current.state.T2).toBe(8);
  });

  it('spawn adds a window and focuses it', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    expect(result.current.state.windows).toHaveLength(1);
    expect(result.current.state.focusedWindowId).toBe(result.current.state.windows[0].windowId);
  });

  it('spawn beyond T2 tucks oldest non-focused', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    for (let i = 0; i < 9; i++) {
      await act(async () => {
        await result.current.spawnWindow();
      });
    }
    const tucked = result.current.state.windows.filter((w) => w.isTucked);
    const onBoard = result.current.state.windows.filter((w) => !w.isTucked);
    expect(onBoard.length).toBe(8);
    expect(tucked.length).toBe(1);
  });

  it('closeWindow drops the window and re-focuses most recent', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
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

  it('tuckWindow removes from board, evokeWindow puts it back', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.tuckWindow(id));
    expect(result.current.state.windows[0].isTucked).toBe(true);
    act(() => result.current.evokeWindow(id));
    expect(result.current.state.windows[0].isTucked).toBe(false);
    expect(result.current.state.focusedWindowId).toBe(id);
  });

  it('renameWindow persists name', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.renameWindow(id, 'Mass Spec Run'));
    expect(result.current.state.windows[0].name).toBe('Mass Spec Run');
  });

  it('organize clears manual placement', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.moveWindow(id, { x: 100, y: 100 }));
    expect(result.current.state.windows[0].isManuallyPlaced).toBe(true);
    act(() => result.current.organize());
    expect(result.current.state.windows[0].isManuallyPlaced).toBe(false);
    expect(result.current.state.windows[0].position).toBeNull();
  });

  it('lowering T1 then T2 below current on-board count tucks excess', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    for (let i = 0; i < 5; i++) {
      await act(async () => {
        await result.current.spawnWindow();
      });
    }
    expect(result.current.state.windows.filter((w) => !w.isTucked)).toHaveLength(5);
    // T2 ≥ T1 invariant — must lower T1 first to allow T2=3
    act(() => result.current.setT1(3));
    act(() => result.current.setT2(3));
    expect(result.current.state.windows.filter((w) => !w.isTucked)).toHaveLength(3);
  });

  it('clearAll removes all windows', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    act(() => result.current.clearAll());
    expect(result.current.state.windows).toHaveLength(0);
  });
});
