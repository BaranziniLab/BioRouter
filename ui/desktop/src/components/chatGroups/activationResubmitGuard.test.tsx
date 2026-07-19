import { render, screen, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import React, { useEffect, useRef, useState } from 'react';
import { MemoryRouter, Routes, Route, useNavigate, useLocation } from 'react-router-dom';
import { ChatGroupsProvider, useChatGroups } from '../../contexts/ChatGroupsContext';
import { createNavigationHandler } from '../../utils/navigationUtils';
import { runInitialMessageAutoSubmit } from '../BaseChat';

/**
 * REGRESSION GUARD — activating a chat tab must NOT re-submit its initial message.
 *
 * Background (scenario "B4"): a suspected regression claimed that switching to a
 * Home-created tab re-submits that tab's `pendingInitialMessage` to the LLM on
 * every remount (only a group's ACTIVE tab is mounted, so a tab switch remounts
 * BaseChat). This test reproduces the FULL live path the existing suites stub
 * away, and pins that HEAD is correct.
 *
 * Fidelity vs the existing tests:
 *   - duplicateSubmissionWiring.test.tsx uses a SYNCHRONOUS BaseChat stub and an
 *     open/close cycle; it mocks the whole ChatGroupsContext.
 *   - homeSubmitWiring.test.tsx only checks the cargo is DELIVERED, never submitted.
 *   - This test drives the REAL ChatGroupsProvider, the REAL useChatGroupsUrlSync
 *     (IN + OUT), the REAL chatGroupsReducer and localStorage persistence, plus a
 *     BaseChat model that faithfully mirrors the real one: ASYNC session load, the
 *     real `runInitialMessageAutoSubmit`, the real `clearRouterState` navigate, an
 *     `onInitialMessageConsumed` -> dispatch(consumePending), a per-mount
 *     hasAutoSubmitted ref reset on the sessionId prop, and "only the active tab is
 *     mounted" (BaseChat keyed by tabId).
 *
 * The durable guard is ChatGroupsShell dispatching `consumePending`, which clears
 * the tab's cargo the instant it is first submitted; every later remount then sees
 * initialMessage=undefined. The SANITY test proves this harness is not vacuous:
 * remove the consumePending dispatch and the same drive DOES re-submit.
 *
 * Verified live at HEAD (isolated Electron): three Home-created tabs, ~15 tab
 * activations plus a pre-session-load race, every session stayed at exactly one
 * user message. The auto-submit/cargo code is byte-identical to the commit where
 * this scenario was reported passing, so no streaming-track commit changed it.
 */

vi.mock('../../hooks/chatStreamStore', () => ({
  useRunningChats: () => [],
}));

const submitLog: string[] = [];

/** When true, the shell "forgets" to spend the cargo — the un-guarded world. */
let breakGuard = false;

/**
 * Faithful stand-in for BaseChat's initial-message auto-submit, incl. the async
 * session load and the real clearRouterState navigation.
 */
function FakeBaseChat({
  sessionId,
  initialMessage,
  onConsumed,
}: {
  sessionId: string;
  initialMessage?: string;
  onConsumed?: () => void;
}) {
  const navigate = useNavigate();
  const location = useLocation();
  const hasAutoSubmitted = useRef(false);
  const [loaded, setLoaded] = useState(false);

  // Mirrors BaseChat "Reset auto-submit flag when session changes" — dep [sessionId].
  useEffect(() => {
    hasAutoSubmitted.current = false;
    setLoaded(false);
  }, [sessionId]);

  // Async session load: the session resolves a tick after sessionId is present.
  useEffect(() => {
    if (!sessionId) {
      setLoaded(false);
      return;
    }
    let live = true;
    Promise.resolve().then(() => {
      if (live) setLoaded(true);
    });
    return () => {
      live = false;
    };
  }, [sessionId]);

  // The real auto-submit effect (BaseChat.tsx).
  useEffect(() => {
    hasAutoSubmitted.current = runInitialMessageAutoSubmit({
      hasSession: loaded,
      hasAutoSubmitted: hasAutoSubmitted.current,
      initialMessage,
      initialAttachments: undefined,
      shouldStartAgent: false,
      submit: (t: string) => submitLog.push(t),
      clearRouterState: () =>
        navigate(location.pathname + location.search, {
          replace: true,
          state: {
            ...(location.state as object),
            initialMessage: undefined,
            initialAttachments: undefined,
          },
        }),
      onConsumed,
    });
  });

  return (
    <div data-testid="basechat" data-initial={initialMessage ?? ''} data-session={sessionId} />
  );
}

/** Mirrors ChatGroupsShell.renderGroup for a single group. */
function Shell() {
  const groups = useChatGroups()!;
  const group = groups.activeGroup;
  const activeTab = groups.activeTab;
  if (!group) return null;
  return (
    <div>
      {group.tabs.map((t) => (
        <button
          key={t.tabId}
          data-testid={`activate-${t.tabId}`}
          onClick={() => groups.dispatch({ type: 'activateTab', tabId: t.tabId })}
        >
          {t.tabId}
        </button>
      ))}
      <button
        data-testid="new-tab"
        onClick={() => groups.dispatch({ type: 'openTab', payload: { sessionId: '' } })}
      >
        +
      </button>
      {activeTab && (
        <FakeBaseChat
          key={activeTab.tabId}
          sessionId={activeTab.sessionId}
          initialMessage={activeTab.pendingInitialMessage}
          onConsumed={
            breakGuard
              ? undefined
              : () => groups.dispatch({ type: 'consumePending', tabId: activeTab.tabId })
          }
        />
      )}
    </div>
  );
}

/** Stands in for the Home surface: same setView('pair', …) the real Home uses. */
function HomeStub({ sessionId, message }: { sessionId: string; message: string }) {
  const navigate = useNavigate();
  const setView = createNavigationHandler(navigate);
  return (
    <button
      data-testid="home-submit"
      onClick={() =>
        setView('pair', {
          resumeSessionId: sessionId,
          initialMessage: message,
          initialAttachments: [],
        })
      }
    >
      send
    </button>
  );
}

function renderApp(strict: boolean) {
  const tree = (
    <MemoryRouter initialEntries={['/']}>
      <ChatGroupsProvider>
        <Routes>
          <Route path="/" element={<HomeStub sessionId="session-A" message="B4X metal" />} />
          <Route path="/pair" element={<Shell />} />
        </Routes>
      </ChatGroupsProvider>
    </MemoryRouter>
  );
  return render(strict ? <React.StrictMode>{tree}</React.StrictMode> : tree);
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

function activeTabIds(): string[] {
  return [...document.querySelectorAll('[data-testid^="activate-"]')].map(
    (b) => b.getAttribute('data-testid')!.replace('activate-', '')
  );
}

/** Home submit -> open a blank tab -> switch back twice; returns the submit log. */
async function driveHomeSubmitThenActivations(strict: boolean) {
  submitLog.length = 0;
  localStorage.clear();
  sessionStorage.clear();
  renderApp(strict);

  await act(async () => {
    screen.getByTestId('home-submit').click();
  });
  await flush();
  await flush();
  const afterFirst = [...submitLog];
  const tab1 = activeTabIds()[0];

  await act(async () => {
    screen.getByTestId('new-tab').click();
  });
  await flush();
  await act(async () => {
    screen.getByTestId(`activate-${tab1}`).click();
  });
  await flush();
  await flush();

  const blankId = activeTabIds().find((id) => id !== tab1)!;
  await act(async () => {
    screen.getByTestId(`activate-${blankId}`).click();
  });
  await flush();
  await act(async () => {
    screen.getByTestId(`activate-${tab1}`).click();
  });
  await flush();
  await flush();

  return { afterFirst, final: [...submitLog] };
}

describe('activating a chat tab does not re-submit its initial message', () => {
  beforeEach(() => {
    submitLog.length = 0;
    breakGuard = false;
  });

  it('submits once, then never again across tab activations', async () => {
    const r = await driveHomeSubmitThenActivations(false);
    expect(r.afterFirst).toEqual(['B4X metal']);
    expect(r.final).toEqual(['B4X metal']);
  });

  it('holds under StrictMode (the app renders under React.StrictMode)', async () => {
    const r = await driveHomeSubmitThenActivations(true);
    expect(r.afterFirst).toEqual(['B4X metal']);
    expect(r.final).toEqual(['B4X metal']);
  });

  /**
   * Proves the harness is not vacuous: with the consumePending dispatch removed
   * (the un-guarded world) the cargo persists and each activation-remount
   * re-submits — which is exactly the failure the guard prevents.
   */
  it('SANITY: without consumePending the cargo persists and activation DOES re-submit', async () => {
    breakGuard = true;
    const r = await driveHomeSubmitThenActivations(true);
    expect(r.final.length).toBeGreaterThan(1);
  });
});
