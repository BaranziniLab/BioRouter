import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { DashboardProvider } from './DashboardProvider';
import { useDashboard } from '../../contexts/DashboardContext';

/**
 * Hardening tests for the dashboard session lifecycle — the heart of the
 * "diverge in dashboard / data-loss" bug fixes:
 *  - Closing a resumed/diverged window must NEVER delete the conversation
 *    (it only stops the in-memory agent).
 *  - Closing a created-here window deletes the session ONLY if it's empty.
 *  - clearAll follows the same rules.
 *  - Stale windows (deleted session → 404) are dropped on hydrate.
 */

let createSessionCounter = 0;
vi.mock('../../sessions', () => ({
  createSession: vi.fn(async () => ({ id: `created_${createSessionCounter++}` })),
}));
vi.mock('../../utils/workingDir', () => ({ getInitialWorkingDir: () => '/tmp' }));

const mockDeleteSession = vi.fn(async () => ({ data: {}, error: undefined }));
const mockStopAgent = vi.fn(async () => ({ data: {}, error: undefined }));
// Default: a non-empty session (so created-here windows are kept unless a test
// overrides the message count).
const mockGetSession = vi.fn(async (_opts: { path: { session_id: string } }) => ({
  data: { message_count: 3 },
  error: undefined,
  response: { status: 200 } as Response,
}));
vi.mock('../../api', () => ({
  deleteSession: (...a: unknown[]) => mockDeleteSession(...(a as [])),
  getSession: (...a: unknown[]) => mockGetSession(...(a as [{ path: { session_id: string } }])),
  stopAgent: (...a: unknown[]) => mockStopAgent(...(a as [])),
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <DashboardProvider>{children}</DashboardProvider>
);

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  localStorage.clear();
  createSessionCounter = 0;
  mockDeleteSession.mockClear();
  mockStopAgent.mockClear();
  mockGetSession.mockClear();
  mockGetSession.mockResolvedValue({
    data: { message_count: 3 },
    error: undefined,
    response: { status: 200 } as Response,
  });
});

describe('dashboard session lifecycle (diverge hardening)', () => {
  it('spawn without resume marks createdHere=true; with resume marks false', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow(); // fresh
    });
    await act(async () => {
      await result.current.spawnWindow({ resumeSessionId: 'diverged_1' }); // resumed/diverged
    });
    const [fresh, resumed] = result.current.state.windows;
    expect(fresh.createdHere).toBe(true);
    expect(resumed.createdHere).toBe(false);
    expect(resumed.sessionId).toBe('diverged_1');
  });

  it('closing a DIVERGED window stops the agent but NEVER deletes the session', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow({ resumeSessionId: 'diverged_42' });
    });
    const id = result.current.state.windows[0].windowId;
    await act(async () => {
      result.current.closeWindow(id);
      await flush();
    });
    expect(result.current.state.windows).toHaveLength(0);
    expect(mockStopAgent).toHaveBeenCalledWith({
      body: { session_id: 'diverged_42' },
      throwOnError: false,
    });
    expect(mockDeleteSession).not.toHaveBeenCalled();
    // Diverged sessions are never even probed for emptiness.
    expect(mockGetSession).not.toHaveBeenCalled();
  });

  it('closing a created-here EMPTY window deletes that throwaway session', async () => {
    mockGetSession.mockResolvedValue({
      data: { message_count: 0 },
      error: undefined,
      response: { status: 200 } as Response,
    });
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const { windowId, sessionId } = result.current.state.windows[0];
    await act(async () => {
      result.current.closeWindow(windowId);
      await flush();
    });
    expect(mockDeleteSession).toHaveBeenCalledWith({
      path: { session_id: sessionId },
      throwOnError: false,
    });
  });

  it('closing a created-here USED window keeps history (stopAgent, no delete)', async () => {
    mockGetSession.mockResolvedValue({
      data: { message_count: 7 },
      error: undefined,
      response: { status: 200 } as Response,
    });
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const { windowId, sessionId } = result.current.state.windows[0];
    await act(async () => {
      result.current.closeWindow(windowId);
      await flush();
    });
    expect(mockDeleteSession).not.toHaveBeenCalled();
    expect(mockStopAgent).toHaveBeenCalledWith({
      body: { session_id: sessionId },
      throwOnError: false,
    });
  });

  it('clearAll preserves diverged sessions (stopAgent only) and removes all windows', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow({ resumeSessionId: 'd1' });
      await result.current.spawnWindow({ resumeSessionId: 'd2' });
    });
    await act(async () => {
      result.current.clearAll();
      await flush();
    });
    expect(result.current.state.windows).toHaveLength(0);
    expect(mockDeleteSession).not.toHaveBeenCalled();
    expect(mockStopAgent).toHaveBeenCalledWith({
      body: { session_id: 'd1' },
      throwOnError: false,
    });
    expect(mockStopAgent).toHaveBeenCalledWith({
      body: { session_id: 'd2' },
      throwOnError: false,
    });
  });

  it('hydrate drops a window whose session 404s, keeps the live one', async () => {
    // Seed two persisted windows: one alive, one whose session was deleted.
    localStorage.setItem(
      'biorouter.dashboard.v2',
      JSON.stringify({
        version: 2,
        windows: [
          {
            windowId: 'dw_alive',
            sessionId: 'alive',
            createdHere: false,
            name: 'Alive',
            userSetName: false,
            badge: 1,
            accentColor: '#000',
            position: { x: 0, y: 0 },
            size: { w: 520, h: 440 },
            isManuallyPlaced: true,
            lastInteraction: 1,
            unreadActivity: false,
            folded: false,
          },
          {
            windowId: 'dw_dead',
            sessionId: 'dead',
            createdHere: false,
            name: 'Dead',
            userSetName: false,
            badge: 2,
            accentColor: '#111',
            position: { x: 600, y: 0 },
            size: { w: 520, h: 440 },
            isManuallyPlaced: true,
            lastInteraction: 2,
            unreadActivity: false,
            folded: false,
          },
        ],
        focusedWindowId: 'dw_dead',
        cameraOffset: { x: 0, y: 0 },
        foldMode: false,
      })
    );
    mockGetSession.mockImplementation(async (opts: { path: { session_id: string } }) => {
      if (opts.path.session_id === 'dead') {
        return { data: undefined, error: {}, response: { status: 404 } as Response };
      }
      return { data: { message_count: 2 }, error: undefined, response: { status: 200 } as Response };
    });

    const { result } = renderHook(() => useDashboard(), { wrapper });
    // Starts with both (synchronous hydrate), then the async filter drops dead.
    expect(result.current.state.windows).toHaveLength(2);
    await act(async () => {
      await flush();
    });
    expect(result.current.state.windows.map((w) => w.sessionId)).toEqual(['alive']);
    // Focus moved off the dropped window.
    expect(result.current.state.focusedWindowId).not.toBe('dw_dead');
  });

  it('hydrate KEEPS windows when the backend errors transiently (no 404)', async () => {
    localStorage.setItem(
      'biorouter.dashboard.v2',
      JSON.stringify({
        version: 2,
        windows: [
          {
            windowId: 'dw_x',
            sessionId: 'maybe',
            createdHere: false,
            name: 'Maybe',
            userSetName: false,
            badge: 1,
            accentColor: '#000',
            position: { x: 0, y: 0 },
            size: { w: 520, h: 440 },
            isManuallyPlaced: true,
            lastInteraction: 1,
            unreadActivity: false,
            folded: false,
          },
        ],
        focusedWindowId: null,
        cameraOffset: { x: 0, y: 0 },
        foldMode: false,
      })
    );
    // Simulate a network rejection (backend not ready at launch).
    mockGetSession.mockRejectedValue(new Error('ECONNREFUSED'));

    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await flush();
    });
    // Window must survive — we never drop on anything but a confirmed 404.
    expect(result.current.state.windows).toHaveLength(1);
  });
});
