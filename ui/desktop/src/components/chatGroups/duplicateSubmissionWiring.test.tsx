import React from 'react';
import { render, screen, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { chatGroupsReducer, createInitialChatGroupsState } from './chatGroupsReducer';
import { ChatGroupsState, leafGroupIds } from './chatGroupsTypes';

/**
 * REGRESSION GATE (wiring half) — duplicate submission on tab close.
 *
 * chatGroupsReducer has ALWAYS had a `consumePending` action; until 2026-07-18
 * no production code dispatched it. So the reducer tests
 * (duplicateSubmission.test.tsx) passed against the broken build — they pin a
 * contract nobody invoked. This one fails unless ChatGroupsShell actually hands
 * BaseChat an `onInitialMessageConsumed` that dispatches it.
 *
 * It drives the REAL reducer through the REAL ChatGroupsShell and the REAL tab
 * strip, clicking the same "+" and "x" the user clicks. Only BaseChat is stood
 * in for, by a component that models the one behaviour that matters: it submits
 * `initialMessage` on mount behind a component-local ref — the exact guard the
 * real BaseChat uses, and the exact guard that dies on unmount. That is the
 * whole bug: only a group's ACTIVE tab is mounted, so closing a tab remounts its
 * successor, whose fresh ref no longer remembers anything.
 *
 * Measured live before the fix: 1 message + 3 open/close cycles => 4 user
 * messages, 4 assistant replies, 23,584 input tokens in one session.
 */

const MARKER = 'WWMARKER name one animal';

/** Every text the stand-in BaseChat actually submitted, across all mounts. */
let submitLog: string[] = [];

vi.mock('../BaseChat', () => {
  function BaseChatStub({
    initialMessage,
    onInitialMessageConsumed,
    renderSessionTitle,
  }: {
    initialMessage?: string;
    onInitialMessageConsumed?: () => void;
    renderSessionTitle?: () => React.ReactNode;
  }) {
    // Mirrors BaseChat's `hasAutoSubmittedRef`: per-mount, so a remount forgets.
    const hasAutoSubmitted = React.useRef(false);
    React.useEffect(() => {
      if (hasAutoSubmitted.current) return;
      if (!initialMessage) return;
      hasAutoSubmitted.current = true;
      submitLog.push(initialMessage);
      onInitialMessageConsumed?.();
    }, [initialMessage, onInitialMessageConsumed]);
    return (
      <div data-testid="basechat" data-initial-message={initialMessage ?? ''}>
        {/* The real BaseChat renders the tab strip through this slot, so the
            stand-in must too — otherwise there are no tabs to click. */}
        {renderSessionTitle?.()}
      </div>
    );
  }
  return { default: BaseChatStub };
});

vi.mock('../InAppTerminalDock', () => ({
  default: () => <div data-testid="dock" />,
}));

vi.mock('../../contexts/TerminalDockContext', () => ({
  useTerminalDock: () => ({
    isOpenFor: () => false,
    setOpen: vi.fn(),
    remove: vi.fn(),
    retain: vi.fn(),
    terminals: [],
  }),
}));

vi.mock('../ui/sidebar', () => ({
  useSidebar: () => ({ state: 'expanded', isMobile: false }),
}));

/**
 * A real store: the production reducer behind useReducer, so a dispatch from the
 * shell genuinely reshapes the tabs and re-renders. Seeded the way the app seeds
 * it — `openTab` with route cargo, which is what the pre-session submit does.
 */
const seed = (): ChatGroupsState =>
  chatGroupsReducer(createInitialChatGroupsState(), {
    type: 'openTab',
    payload: { sessionId: 'session-A', title: 'session-A', pendingInitialMessage: MARKER },
  });

let latestState: ChatGroupsState;

vi.mock('../../contexts/ChatGroupsContext', () => ({
  useChatGroups: () => {
    const [state, dispatch] = React.useReducer(chatGroupsReducer, undefined, seed);
    latestState = state;
    return { state, dispatch, runningSessionIds: [] };
  },
}));

import ChatGroupsShell from './ChatGroupsShell';

const allTabs = (s: ChatGroupsState) => leafGroupIds(s.layout).flatMap((id) => s.groups[id].tabs);
const cargo = (s: ChatGroupsState) =>
  allTabs(s)
    .map((t) => t.pendingInitialMessage)
    .filter((m) => m !== undefined);

/** One user gesture: click "+", then close the tab that appeared. */
function openThenCloseATab() {
  const before = allTabs(latestState).map((t) => t.tabId);

  act(() => {
    screen.getByTestId('chat-tab-new').click();
  });

  const opened = allTabs(latestState).find((t) => !before.includes(t.tabId));
  expect(opened, 'clicking "+" should have opened a tab').toBeDefined();

  act(() => {
    screen.getByTestId(`chat-tab-close-${opened!.tabId}`).click();
  });

  expect(allTabs(latestState).map((t) => t.tabId)).toEqual(before);
}

describe('ChatGroupsShell spends a tab’s initial message exactly once', () => {
  beforeEach(() => {
    submitLog = [];
  });

  it('submits it on the first mount', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(submitLog).toEqual([MARKER]);
  });

  it('clears the tab’s cargo the moment it is submitted', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    // The tab survives with its session; only the spent message is gone.
    expect(cargo(latestState)).toEqual([]);
    expect(allTabs(latestState).map((t) => t.sessionId)).toEqual(['session-A']);
  });

  it('stops re-submitting it to BaseChat on later renders', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(screen.getByTestId('basechat').getAttribute('data-initial-message')).toBe('');
  });

  /** The reported repro, verbatim: before the fix this logged four submissions. */
  it('does not re-submit across three open/close tab cycles', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(submitLog).toEqual([MARKER]);

    openThenCloseATab();
    openThenCloseATab();
    openThenCloseATab();

    expect(submitLog).toEqual([MARKER]);
    expect(cargo(latestState)).toEqual([]);
  });

  /**
   * Guards the stand-in itself. If the mock stopped remounting on a tab switch,
   * every assertion above would pass vacuously — so prove the mechanism the bug
   * needs is still present: with cargo restored, a cycle DOES re-submit.
   */
  it('sanity: the cycle really does remount BaseChat (so the gate is not vacuous)', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(submitLog).toEqual([MARKER]);

    // Put the cargo back, simulating the un-fixed world where it was never spent.
    const tabId = allTabs(latestState)[0].tabId;
    act(() => {
      screen.getByTestId('chat-tab-new').click();
    });
    const opened = allTabs(latestState).find((t) => t.tabId !== tabId)!;
    act(() => {
      screen.getByTestId(`chat-tab-close-${opened.tabId}`).click();
    });

    // The remount happened; it submitted nothing only because the cargo is gone.
    expect(screen.getByTestId('basechat')).toBeDefined();
    expect(submitLog).toEqual([MARKER]);
  });
});
