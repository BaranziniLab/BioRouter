import { render, act, waitFor } from '@testing-library/react';
import { useEffect } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';
import { requestNewTab, hasPendingNewTab, resetNewTabRegistry } from './newTabRegistry';
import { closeActiveTab, resetCloseActiveTabRegistry } from './closeActiveTabRegistry';

/**
 * Issue #38 — no tabs → Home. The empty "New Session" pane must never be the
 * resting state: /pair with a zero-tab layout and no pending cargo redirects
 * to '/' (the Hub). These tests drive the REAL ChatGroupsProvider so the
 * effect-ordering hazard is genuinely exercised: the redirect hook runs in a
 * CHILD of the provider, so its effect fires BEFORE the provider's mount
 * effects (URL-open, pending-Cmd+T consumption) — every gate is load-bearing,
 * and a missing one bounces a legitimate deep link or Cmd+T back Home.
 *
 * Keyboard entry points drive the registries directly (requestNewTab /
 * closeActiveTab): the Electron menu owns Cmd+T/Cmd+W, so the keystrokes never
 * reach the DOM — same rationale as keyboardResubmitGuard.test.tsx.
 */

vi.mock('../../hooks/chatStreamStore', () => ({ useRunningChats: () => [] }));
vi.mock('../../utils/sessionNameSync', () => ({
  subscribeSessionNameChanges: () => () => undefined,
}));

import { ChatGroupsProvider, useChatGroups } from '../../contexts/ChatGroupsContext';
import { useEmptyPairRedirect } from './useEmptyPairRedirect';
import { ChatGroupsState, leafGroupIds } from './chatGroupsTypes';
import { ChatGroupsAction } from './chatGroupsReducer';

let currentPath = '';
let latestState: ChatGroupsState | null = null;
let latestDispatch: ((action: ChatGroupsAction) => void) | null = null;

function LocationSpy() {
  const location = useLocation();
  currentPath = location.pathname + location.search;
  return null;
}

/** Stands in for PairRouteContent: publishes state and calls the hook. */
function Probe({ isCreatingSession = false }: { isCreatingSession?: boolean }) {
  const ctx = useChatGroups();
  latestState = ctx?.state ?? null;
  latestDispatch = ctx?.dispatch ?? null;
  useEmptyPairRedirect(isCreatingSession);
  return <div data-testid="pair-surface" />;
}

/**
 * Fires a Cmd+T on the ZERO-TAB commit, from an effect that runs BEFORE the
 * redirect effect (a preceding sibling's effects flush first). This is the
 * deterministic model of Codex B6 finding 2's interleaving: in the app the
 * commit and its passive-effect flush are separate scheduler tasks, so the
 * Cmd+T IPC can land between them — here the injector IS that gap. The
 * redirect effect then runs with a zero-tab closure and a freshly dispatched,
 * not-yet-committed tab.
 */
let injectCmdTOnZeroTabs = false;
function CmdTInjector() {
  const ctx = useChatGroups();
  const tabCount = ctx
    ? leafGroupIds(ctx.state.layout).reduce(
        (count, id) => count + ctx.state.groups[id].tabs.length,
        0
      )
    : 0;
  useEffect(() => {
    if (injectCmdTOnZeroTabs && tabCount === 0) {
      injectCmdTOnZeroTabs = false;
      expect(requestNewTab()).toBe(true); // a live provider dispatches directly
    }
  }, [tabCount]);
  return null;
}

function mountAt(
  entry: string | { pathname: string; search?: string; state?: unknown },
  { isCreatingSession = false }: { isCreatingSession?: boolean } = {}
) {
  return render(
    <MemoryRouter initialEntries={[entry as never]}>
      <LocationSpy />
      <Routes>
        <Route path="/" element={<div data-testid="home" />} />
        <Route
          path="/pair"
          element={
            <ChatGroupsProvider>
              <CmdTInjector />
              <Probe isCreatingSession={isCreatingSession} />
            </ChatGroupsProvider>
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

const allTabs = (s: ChatGroupsState | null) =>
  s ? leafGroupIds(s.layout).flatMap((id) => s.groups[id].tabs) : [];

describe('useEmptyPairRedirect — no tabs → Home (issue #38)', () => {
  beforeEach(() => {
    localStorage.clear();
    currentPath = '';
    latestState = null;
    latestDispatch = null;
    injectCmdTOnZeroTabs = false;
    resetNewTabRegistry();
    resetCloseActiveTabRegistry();
  });

  afterEach(() => {
    delete (window as { appConfig?: unknown }).appConfig;
  });

  it('a stale /pair deep link with zero tabs and no cargo lands on Home', async () => {
    const { queryByTestId } = mountAt('/pair');
    await waitFor(() => expect(currentPath).toBe('/'));
    expect(queryByTestId('home')).toBeTruthy();
    expect(queryByTestId('pair-surface')).toBeNull();
  });

  it('closing the LAST tab returns to Home', async () => {
    mountAt('/pair?resumeSessionId=session-A');
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));

    // Cmd+W, through the registry the menu's IPC handler calls.
    let claimed = false;
    act(() => {
      claimed = closeActiveTab();
    });
    expect(claimed).toBe(true);

    await waitFor(() => expect(currentPath).toBe('/'));
  });

  it('a /pair?resumeSessionId= deep link is NOT bounced — the tab opens', async () => {
    const { queryByTestId } = mountAt('/pair?resumeSessionId=session-X');
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));
    expect(allTabs(latestState)[0].sessionId).toBe('session-X');
    expect(currentPath).toContain('/pair');
    expect(queryByTestId('home')).toBeNull();
  });

  it('REGRESSION (Codex B6.2): Cmd+T racing the last-tab close is not bounced Home', async () => {
    // The interleaving: a zero-tab state COMMITS, the Cmd+T IPC lands and
    // dispatches through the live provider's handler, and only THEN does the
    // zero-tab commit's redirect effect run. It sees tabCount 0 — it must see
    // the in-flight dispatch via hasPendingNewTab() and stand down, or the
    // window goes Home and the freshly dispatched tab dies with the
    // unmounting provider. CmdTInjector (mounted before the probe, so its
    // effects flush first) plays the IPC's part deterministically.
    //
    // The open tab must be a BLANK one (clean URL): a ?resumeSessionId= tab
    // would leave its param in the URL across the close and gate the stale
    // effect for the wrong reason, masking the race.
    act(() => {
      expect(requestNewTab()).toBe(false); // remembered; the mount consumes it
    });
    mountAt('/pair');
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));
    expect(currentPath).toBe('/pair'); // no URL cargo — only the peek can gate

    act(() => {
      injectCmdTOnZeroTabs = true; // the IPC will land in the effect gap
      expect(closeActiveTab()).toBe(true);
    });
    expect(injectCmdTOnZeroTabs).toBe(false); // the injector really fired

    // The keystroke won: one blank tab, still on /pair.
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));
    expect(allTabs(latestState)[0].sessionId).toBe('');
    expect(currentPath).toContain('/pair');

    // The nonzero commit acknowledged the request, so it cannot suppress a
    // LATER legitimate redirect: closing this tab still goes Home.
    expect(hasPendingNewTab()).toBe(false);
    act(() => {
      latestDispatch?.({ type: 'closeTab', tabId: allTabs(latestState)[0].tabId });
    });
    await waitFor(() => expect(currentPath).toBe('/'));
  });

  it('Cmd+T from Settings survives: the redirect PEEKS the pending request, never consumes it', async () => {
    // Cmd+T with no provider mounted: remembered, then App.tsx navigates to
    // /pair. The redirect effect runs BEFORE the provider's consuming effect
    // (child before parent) — it must stand down without eating the request.
    act(() => {
      expect(requestNewTab()).toBe(false);
    });
    expect(hasPendingNewTab()).toBe(true);

    mountAt('/pair');

    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));
    expect(allTabs(latestState)[0].sessionId).toBe(''); // one blank tab
    expect(currentPath).toContain('/pair');
    expect(hasPendingNewTab()).toBe(false); // the provider cashed it in
  });

  it('the sidebar new-chat navigation (route state newChat) is not bounced', async () => {
    // PairRouteContent opens the blank tab in its own effect; the gate must
    // hold even while the layout is momentarily empty.
    const { queryByTestId } = mountAt({ pathname: '/pair', state: { newChat: true } });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair');
    expect(queryByTestId('pair-surface')).toBeTruthy();
  });

  it('a workflow deep link param holds the surface while the session is created', async () => {
    const { queryByTestId } = mountAt({ pathname: '/pair', search: '?workflowId=wf-1' });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair?workflowId=wf-1');
    expect(queryByTestId('pair-surface')).toBeTruthy();
  });

  it('a workflow deeplink from appConfig holds the surface', async () => {
    (window as { appConfig?: { get: (k: string) => unknown } }).appConfig = {
      get: (key: string) => (key === 'workflowDeeplink' ? 'biorouter://workflow/x' : undefined),
    };
    mountAt('/pair');
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair');
  });

  it('an initialMessage in route state holds the surface (session creation is en route)', async () => {
    mountAt({ pathname: '/pair', state: { initialMessage: 'hello there' } });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair');
  });

  it('a FRESH WINDOW awaiting its launcher message (?initialMessagePending=) is not bounced', async () => {
    // The launcher cargo cannot ride the URL: main.ts parks it in the main
    // process and delivers it as set-initial-message IPC only after App.tsx
    // signals react-ready — an effect that runs AFTER this child hook's. The
    // window's first commit is therefore a bare zero-tab /pair with NO route
    // state; only the synchronous URL marker set at window creation can hold
    // the surface until the cargo lands.
    const { queryByTestId } = mountAt({
      pathname: '/pair',
      search: '?initialMessagePending=true',
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair?initialMessagePending=true');
    expect(queryByTestId('pair-surface')).toBeTruthy();
    expect(queryByTestId('home')).toBeNull();
  });

  it('mid-flight session creation (isCreatingSession) holds the surface', async () => {
    mountAt('/pair', { isCreatingSession: true });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toBe('/pair');
  });

  it('emptying ONE half of a split collapses the pane and stays on /pair', async () => {
    mountAt('/pair?resumeSessionId=session-A');
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(1));

    // Build a split: open a second tab and carry it to the right half.
    act(() => {
      latestDispatch?.({ type: 'openTab', payload: { sessionId: 'session-B' } });
    });
    await waitFor(() => expect(allTabs(latestState)).toHaveLength(2));
    const state = latestState as ChatGroupsState;
    const groupId = leafGroupIds(state.layout)[0];
    const tabB = allTabs(state).find((t) => t.sessionId === 'session-B');
    act(() => {
      latestDispatch?.({
        type: 'moveTabToGroup',
        tabId: tabB!.tabId,
        targetGroupId: groupId,
        zone: 'right',
      });
    });
    await waitFor(() => expect(leafGroupIds(latestState!.layout)).toHaveLength(2));

    // Empty the right half: it collapses out of the tree; the survivor keeps
    // the layout non-empty, so NO redirect fires.
    act(() => {
      latestDispatch?.({ type: 'closeTab', tabId: tabB!.tabId });
    });
    await waitFor(() => expect(leafGroupIds(latestState!.layout)).toHaveLength(1));
    expect(allTabs(latestState)).toHaveLength(1);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(currentPath).toContain('/pair');

    // …and closing the true last tab still goes Home.
    act(() => {
      latestDispatch?.({ type: 'closeTab', tabId: allTabs(latestState)[0].tabId });
    });
    await waitFor(() => expect(currentPath).toBe('/'));
  });
});
