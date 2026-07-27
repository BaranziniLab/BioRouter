import { render, screen, act, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { MemoryRouter, Routes, Route, useNavigate, useLocation } from 'react-router-dom';
import { ChatGroupsProvider, useChatGroups } from '../../contexts/ChatGroupsContext';
import { deliverLauncherMessage } from '../../utils/launcherMessage';

/**
 * REGRESSION GATE — a message typed in the LAUNCHER reaches the tab (#38,
 * Codex B6 re-review findings 1 + 2).
 *
 * The flow under test: main.ts opens a fresh window on
 * `/pair?initialMessagePending=true` (the marker that holds the empty-pair
 * redirect open) with the message parked in the main process; after react-ready
 * the `set-initial-message` IPC fires and App.tsx calls deliverLauncherMessage,
 * which creates the session and navigates.
 *
 * The bug (finding 1): that navigation carried the session id only in
 * location.state. But /pair's deep-link inbox — useChatGroupsUrlSync's IN
 * effect — is gated on `searchParams.get('resumeSessionId')` and returns early
 * when the param is absent, BEFORE reading location.state. So no tab was ever
 * opened and the launcher's message was stranded: session created, window on a
 * zero-tab /pair, text silently gone. The exact shape of the Home-composer bug
 * homeSubmitWiring.test.tsx gates for createNavigationHandler.
 *
 * Finding 2: the navigation must REPLACE the bootstrap
 * `?initialMessagePending=true` entry. Left in history, Back resurrects a
 * zero-tab /pair whose marker suppresses the empty-pair redirect indefinitely.
 *
 * Like homeSubmitWiring, this drives the REAL chain — the real
 * deliverLauncherMessage, the real react-router, the real useChatGroupsUrlSync,
 * the real ChatGroupsProvider and reducer — and asserts on
 * activeTab.pendingInitialMessage, verbatim what ChatGroupsShell passes to
 * BaseChat as initialMessage. Only createSession (an HTTP call) and
 * useRunningChats (needs the chat-stream registry) are stubbed.
 */

vi.mock('../../hooks/chatStreamStore', () => ({
  useRunningChats: () => [],
}));

vi.mock('../../sessions', () => ({
  createSession: vi.fn().mockResolvedValue({ id: 'session-launcher' }),
}));

/**
 * Stands in for App.tsx's set-initial-message IPC handler: the same
 * deliverLauncherMessage call, triggered by a click instead of the IPC event.
 */
function LauncherIpcStub({ message }: { message: string }) {
  const navigate = useNavigate();
  return (
    <button
      data-testid="launcher-ipc"
      onClick={() => void deliverLauncherMessage(navigate, message)}
    >
      deliver
    </button>
  );
}

/** Mirrors ChatGroupsShell — the exact cargo BaseChat receives. */
function PairStub() {
  const groups = useChatGroups();
  const location = useLocation();
  return (
    <div
      data-testid="pair"
      data-initial-message={groups?.activeTab?.pendingInitialMessage ?? ''}
      data-session-id={groups?.activeTab?.sessionId ?? ''}
      data-search={location.search}
    />
  );
}

/** Back button, so the test can attempt to traverse into the bootstrap entry. */
function BackStub() {
  const navigate = useNavigate();
  return (
    <button data-testid="back" onClick={() => navigate(-1)}>
      back
    </button>
  );
}

function renderLauncherWindow(message: string) {
  // The window main.ts opens: the marker route is the FIRST and ONLY history
  // entry, exactly as loadURL leaves it.
  return render(
    <MemoryRouter initialEntries={['/pair?initialMessagePending=true']}>
      <ChatGroupsProvider>
        <Routes>
          <Route
            path="/pair"
            element={
              <>
                <LauncherIpcStub message={message} />
                <PairStub />
                <BackStub />
              </>
            }
          />
        </Routes>
      </ChatGroupsProvider>
    </MemoryRouter>
  );
}

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});

describe('a launcher message reaches the chat tab (set-initial-message → tab)', () => {
  it('creates the tab for the new session with the message as its cargo', async () => {
    renderLauncherWindow('WWMARKER launcher says hi');

    act(() => {
      screen.getByTestId('launcher-ipc').click();
    });

    // The whole finding-1 bug in two assertions: with a state-only navigation
    // the URL-sync gate never opens, no tab exists, and both read ''.
    await waitFor(() => {
      const pair = screen.getByTestId('pair');
      expect(pair.getAttribute('data-session-id')).toBe('session-launcher');
      expect(pair.getAttribute('data-initial-message')).toBe('WWMARKER launcher says hi');
    });
  });

  it('hands the session id off in the query string and drops the bootstrap marker', async () => {
    renderLauncherWindow('hello');

    act(() => {
      screen.getByTestId('launcher-ipc').click();
    });

    await waitFor(() => {
      const search = screen.getByTestId('pair').getAttribute('data-search') ?? '';
      expect(search).toContain('resumeSessionId=session-launcher');
      expect(search).not.toContain('initialMessagePending');
    });
  });

  it('REPLACES the bootstrap entry: Back cannot resurrect ?initialMessagePending=true', async () => {
    renderLauncherWindow('hello');

    act(() => {
      screen.getByTestId('launcher-ipc').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('pair').getAttribute('data-session-id')).toBe('session-launcher');
    });

    // With replace:true the marker entry no longer exists, so Back is a no-op
    // (the stack has one entry). A push would land us straight back on the
    // stale zero-tab marker route.
    act(() => {
      screen.getByTestId('back').click();
    });

    const search = screen.getByTestId('pair').getAttribute('data-search') ?? '';
    expect(search).not.toContain('initialMessagePending');
    expect(search).toContain('resumeSessionId=session-launcher');
  });

  it('a failed session creation navigates nowhere — the marker keeps the window parked', async () => {
    const { createSession } = await import('../../sessions');
    vi.mocked(createSession).mockRejectedValueOnce(new Error('backend down'));
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

    renderLauncherWindow('hello');

    act(() => {
      screen.getByTestId('launcher-ipc').click();
    });

    // The helper logs and swallows the failure; once it has, nothing moved.
    await waitFor(() => expect(consoleError).toHaveBeenCalled());
    const pair = screen.getByTestId('pair');
    expect(pair.getAttribute('data-search')).toContain('initialMessagePending=true');
    expect(pair.getAttribute('data-session-id')).toBe('');
    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });
});
