import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import React from 'react';
import { useDiverge } from './useDiverge';
import { DashboardContext, DashboardApi } from '../contexts/DashboardContext';
import { DashboardCanvasContext } from '../contexts/DashboardCanvasContext';

const mockDivergeSession = vi.fn();
vi.mock('../api', () => ({
  divergeSession: (...args: unknown[]) => mockDivergeSession(...args),
}));
vi.mock('../toasts', () => ({ toastError: vi.fn() }));

const mockCreateChatWindow = vi.fn();
const mockCreateDivergedChatWindow = vi.fn();
const mockSpawnWindow = vi.fn(async () => {});

beforeEach(() => {
  vi.clearAllMocks();
  // @ts-expect-error test shim
  window.electron = {
    createChatWindow: mockCreateChatWindow,
    createDivergedChatWindow: mockCreateDivergedChatWindow,
  };
  mockDivergeSession.mockResolvedValue({ data: { sessionId: 'branch_1', workingDir: '/wd' } });
});

function dashboardApi(): DashboardApi {
  return new Proxy({ spawnWindow: mockSpawnWindow } as Partial<DashboardApi>, {
    get(target, prop) {
      if (prop in target) return target[prop as keyof DashboardApi];
      return () => {};
    },
  }) as DashboardApi;
}

// A: app-wide DashboardProvider present, but the component is NOT on the canvas
// (i.e. the normal chat/Hub/Pair view). This is the exact situation the bug
// lived in — the dashboard context is reachable everywhere.
const chatWrapper = ({ children }: { children: React.ReactNode }) => (
  <DashboardContext.Provider value={dashboardApi()}>{children}</DashboardContext.Provider>
);

// B: rendered inside a dashboard canvas ChatWindow.
const canvasWrapper = ({ children }: { children: React.ReactNode }) => (
  <DashboardContext.Provider value={dashboardApi()}>
    <DashboardCanvasContext.Provider value={true}>{children}</DashboardCanvasContext.Provider>
  </DashboardContext.Provider>
);

describe('useDiverge isolation (chat vs dashboard canvas)', () => {
  it('in a chat view opens a NEW Electron window and does NOT touch the dashboard', async () => {
    const { result } = renderHook(() => useDiverge(), { wrapper: chatWrapper });
    expect(result.current.inDashboard).toBe(false);

    await act(async () => {
      await result.current.diverge('orig');
    });

    expect(mockCreateDivergedChatWindow).toHaveBeenCalledWith('/wd', 'branch_1');
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
    // Critical: the chat-side diverge must NOT spawn into the dashboard.
    expect(mockSpawnWindow).not.toHaveBeenCalled();
  });

  it('on the dashboard canvas spawns an on-canvas window and opens NO Electron window', async () => {
    const { result } = renderHook(() => useDiverge(), { wrapper: canvasWrapper });
    expect(result.current.inDashboard).toBe(true);

    await act(async () => {
      await result.current.diverge('orig');
    });

    expect(mockSpawnWindow).toHaveBeenCalledWith({ resumeSessionId: 'branch_1', cwd: '/wd' });
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
  });

  it('with no dashboard context at all (pure window) opens an Electron window', async () => {
    const { result } = renderHook(() => useDiverge());
    expect(result.current.inDashboard).toBe(false);
    await act(async () => {
      await result.current.diverge('orig');
    });
    expect(mockCreateDivergedChatWindow).toHaveBeenCalled();
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
    expect(mockSpawnWindow).not.toHaveBeenCalled();
  });
});
