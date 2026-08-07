import { act, fireEvent, render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { ChatGroupsState } from './chatGroupsTypes';

/**
 * THE SHELL'S HALF OF TAB TEAR-OFF — the wiring, not the gesture.
 *
 * jsdom cannot verify the gesture: no pointer capture, no layout, no windows,
 * and `document.elementFromPoint` is MISSING (not "returns null"), so the drag
 * hook's move handler throws on the first promoted move. That is browser-
 * verified, per the design's Phase 0.
 *
 * What is verified here is the set of DECISIONS the shell owns and the geometry
 * cannot make for it:
 *   - D5's backstop: the last tab in a window is not torn off, but IS merged
 *   - D6a's ordering: the source drops its tab only on the target's ack, and
 *     `noop` — the answer to every failure — leaves the tab alone
 *   - the merge receive path: caret, insert at the caret's index, acknowledge
 *   - strip bands are reported to main so this window can be merged INTO
 *
 * ⚠ NOTHING HERE ASKS WHETHER A TAB IS RUNNING. Design D1 refused to move a tab
 * with a turn in flight and is superseded (§3.1): the turn stream takes as many
 * subscribers as it likes and the destination window rejoins the turn on load.
 * A test that asserted a running tab could not leave would now be pinning a
 * restriction the codebase deliberately removed.
 */

// The strip is rendered THROUGH BaseChat's session-title slot, so a mock that
// drops `renderSessionTitle` produces a shell with no strip — and the merge half
// of this feature has nothing to hit-test.
vi.mock('../BaseChat', () => ({
  default: ({ renderSessionTitle }: { renderSessionTitle?: () => React.ReactNode }) => (
    <div data-testid="basechat">{renderSessionTitle?.()}</div>
  ),
}));
vi.mock('../InAppTerminalDock', () => ({ default: () => null }));
vi.mock('../ui/sidebar', () => ({ useSidebar: () => ({ state: 'expanded', isMobile: false }) }));
vi.mock('../../contexts/TerminalDockContext', () => ({
  useTerminalDock: () => ({ terminals: [], retain: vi.fn(), setOpen: vi.fn(), remove: vi.fn() }),
}));

const dispatch = vi.fn();
let state: ChatGroupsState;

vi.mock('../../contexts/ChatGroupsContext', () => ({
  useChatGroups: () => ({ dispatch, runningSessionIds: ['sess-1'], state }),
}));

import ChatGroupsShell from './ChatGroupsShell';
import { payloadFromTab } from './tabTearOff';

function stateWith(tabIds: string[]): ChatGroupsState {
  return {
    version: 1,
    layout: { kind: 'leaf', groupId: 'g1' },
    groups: {
      g1: {
        groupId: 'g1',
        activeTabId: tabIds[0] ?? null,
        tabs: tabIds.map((tabId, i) => ({
          tabId,
          sessionId: `sess-${i + 1}`,
          title: `Chat ${i + 1}`,
          userSetName: false,
        })),
      },
    },
    activeGroupId: 'g1',
    seq: tabIds.length,
  } as unknown as ChatGroupsState;
}

/**
 * A stand-in for the preload bridge, plus the two inbound channels. `on` records
 * handlers so a test can play main's side of the protocol.
 */
type Handler = (event: unknown, ...args: unknown[]) => void;
let channels: Map<string, Handler>;
let electron: {
  on: ReturnType<typeof vi.fn>;
  tabDragRegisterBands: ReturnType<typeof vi.fn>;
  tabDragMove: ReturnType<typeof vi.fn>;
  tabDragEnd: ReturnType<typeof vi.fn>;
  tabDragCommit: ReturnType<typeof vi.fn>;
  tabDragAckMerge: ReturnType<typeof vi.fn>;
  closeWindow: ReturnType<typeof vi.fn>;
};

let restoreDomStub: (() => void) | null = null;

beforeEach(() => {
  dispatch.mockReset();
  state = stateWith(['t1', 't2']);
  channels = new Map();
  electron = {
    on: vi.fn((channel: string, handler: Handler) => {
      channels.set(channel, handler);
      return () => channels.delete(channel);
    }),
    tabDragRegisterBands: vi.fn(),
    tabDragMove: vi.fn(),
    tabDragEnd: vi.fn(),
    tabDragCommit: vi.fn().mockResolvedValue({ outcome: 'detach' }),
    tabDragAckMerge: vi.fn(),
    closeWindow: vi.fn(),
  };
  (window as unknown as { electron: unknown }).electron = electron;
  // jsdom has NO elementFromPoint at all. Every merge test that wants a target
  // overrides this; the default answers "over nothing", which is what a real
  // browser answers for a point outside any strip.
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => null;
});

afterEach(() => {
  restoreDomStub?.();
  restoreDomStub = null;
  delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
  delete (window as unknown as { electron?: unknown }).electron;
});

/** Play main's side: deliver a frame on one of the two inbound channels. */
function fromMain(channel: string, payload: unknown) {
  // Wrapped in `act` because a preview frame is a state update arriving from
  // OUTSIDE React — which is the whole point of the target side: this window
  // receives no pointer events, so every one of its updates comes in over IPC.
  act(() => {
    channels.get(channel)?.({}, payload);
  });
}

/**
 * Give the tabs real widths. jsdom measures every box as zero, which collapses
 * every insertion index onto "after the last tab" — a defensible answer, and the
 * wrong one to assert a caret against.
 */
function stubTabWidths(widthPerTab = 100) {
  const original = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function measured(this: Element) {
    const tabId = (this as HTMLElement).dataset?.tabId;
    if (!tabId) return original.call(this);
    const index = tabId === 't1' ? 0 : 1;
    const left = index * widthPerTab;
    return { x: left, left, y: 0, top: 0, width: widthPerTab, height: 34 } as DOMRect;
  };
  return () => {
    Element.prototype.getBoundingClientRect = original;
  };
}

describe('ChatGroupsShell — reporting this window to main', () => {
  it('registers its strip bands, so another window can merge into it', async () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    // Measured one frame late so flex has divided the row before the rects are
    // taken; jsdom's rAF fires on a macrotask.
    await waitFor(() => expect(electron.tabDragRegisterBands).toHaveBeenCalled());
    expect(Array.isArray(electron.tabDragRegisterBands.mock.calls[0][0])).toBe(true);
  });

  it('clears every window’s preview when it unmounts mid-drag', () => {
    const view = render(<ChatGroupsShell onChatChange={() => {}} />);
    electron.tabDragEnd.mockClear();
    view.unmount();
    // A window that dies holding a caret leaves one painted in a window that
    // will never hear from it again.
    expect(electron.tabDragEnd).toHaveBeenCalled();
  });
});

describe('ChatGroupsShell — receiving a merge (the target side)', () => {
  function stripInDocument(container: HTMLElement) {
    const strip = container.querySelector<HTMLElement>('[data-tab-strip-group]');
    if (!strip) throw new Error('the shell rendered no strip');
    (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () =>
      strip;
    return strip;
  }

  it('inserts at the caret’s index and acknowledges, in that order', () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    stripInDocument(container);
    dispatch.mockClear();

    fromMain('tab-drag:merge', {
      requestId: 7,
      tab: { sessionId: 'incoming', title: 'From next door', userSetName: true },
      screenX: 40,
      screenY: 10,
    });

    const openTab = dispatch.mock.calls.find(([action]) => action.type === 'openTab')?.[0];
    expect(openTab.payload).toMatchObject({
      sessionId: 'incoming',
      title: 'From next door',
      userSetName: true,
      groupId: 'g1',
    });
    expect(typeof openTab.payload.index).toBe('number');
    expect(electron.tabDragAckMerge).toHaveBeenCalledWith(7, true);
  });

  it('REFUSES when the point is over no strip — and the source keeps its tab', () => {
    render(<ChatGroupsShell onChatChange={() => {}} />);
    dispatch.mockClear();

    fromMain('tab-drag:merge', {
      requestId: 9,
      tab: { sessionId: 'incoming', title: 'Nowhere', userSetName: false },
      screenX: 40,
      screenY: 4000,
    });

    // `false` is the whole safety property of D6a's round trip: an insert that
    // did not happen must not license a removal.
    expect(electron.tabDragAckMerge).toHaveBeenCalledWith(9, false);
    expect(dispatch.mock.calls.some(([action]) => action.type === 'openTab')).toBe(false);
  });

  it('paints the caret from a preview frame and takes it away again', () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    stripInDocument(container);
    restoreDomStub = stubTabWidths();

    // x=10 is left of the first tab's midpoint, so the caret sits on t1.
    fromMain('tab-drag:preview', { active: true, screenX: 10, screenY: 10 });
    expect(container.querySelector('[data-tab-id="t1"][data-dropbefore="true"]')).toBeTruthy();

    // x=160 is past t2's midpoint: the caret moves off t1 and, being an append,
    // hangs on no tab at all.
    fromMain('tab-drag:preview', { active: true, screenX: 160, screenY: 10 });
    expect(container.querySelector('[data-dropbefore="true"]')).toBeNull();

    fromMain('tab-drag:preview', { active: true, screenX: 110, screenY: 10 });
    expect(container.querySelector('[data-tab-id="t2"][data-dropbefore="true"]')).toBeTruthy();

    fromMain('tab-drag:preview', { active: false });
    expect(container.querySelector('[data-dropbefore="true"]')).toBeNull();
  });

  /**
   * THE DEADLINE, WHICH USED TO BE ONE-SIDED.
   *
   * Main gives up on an unanswered merge and tells the source to KEEP its tab.
   * This window had no deadline at all, so a window busy with a heavy streaming
   * turn could process the request long after that, insert the tab, and
   * acknowledge into a request that no longer existed — source keeps the tab,
   * target inserted it, SAME SESSION IN TWO WINDOWS.
   */
  it('REFUSES an expired merge instead of inserting a tab the source will keep', () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    stripInDocument(container);
    dispatch.mockClear();

    fromMain('tab-drag:merge', {
      requestId: 11,
      tab: { sessionId: 'incoming', title: 'Too late', userSetName: false },
      screenX: 40,
      screenY: 10,
      // Everything else about this request is valid: the point IS over the
      // strip, so without the deadline check it would insert.
      expiresAt: Date.now() - 1,
    });

    expect(dispatch.mock.calls.some(([action]) => action.type === 'openTab')).toBe(false);
    // Answering rather than staying silent ends the source's wait immediately.
    expect(electron.tabDragAckMerge).toHaveBeenCalledWith(11, false);
  });

  it('accepts a merge that is still within its deadline', () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    stripInDocument(container);
    dispatch.mockClear();

    fromMain('tab-drag:merge', {
      requestId: 12,
      tab: { sessionId: 'incoming', title: 'In time', userSetName: false },
      screenX: 40,
      screenY: 10,
      expiresAt: Date.now() + 5000,
    });

    expect(dispatch.mock.calls.some(([action]) => action.type === 'openTab')).toBe(true);
    expect(electron.tabDragAckMerge).toHaveBeenCalledWith(12, true);
  });
});

/**
 * The SOURCE side: press a tab, drag past the window's edge, let go.
 *
 * jsdom cannot say anything about the gesture's geometry, but it can say
 * everything about the shell's decisions once the hook reports "outside" — which
 * is where D5 and D6a live.
 */
describe('ChatGroupsShell — committing a drag that left the window', () => {
  function dragTabOutAndRelease(container: HTMLElement, tabId: string) {
    const tab = container.querySelector<HTMLElement>(`[data-tab-id="${tabId}"] button[role="tab"]`);
    if (!tab) throw new Error(`no tab button for ${tabId}`);
    // `fireEvent`, not `new PointerEvent`: jsdom's PointerEvent constructor
    // drops the MouseEventInit coordinates, so a hand-built event promotes no
    // drag and the whole gesture silently does nothing.
    fireEvent.pointerDown(tab, { button: 0, pointerId: 1, clientX: 40, clientY: 20 });
    // Promote (past the 5px threshold) and land outside the viewport in one
    // move; the hook derives `detach` from the point, not from a sequence.
    fireEvent.pointerMove(window, {
      pointerId: 1,
      clientX: window.innerWidth + 80,
      clientY: 20,
      screenX: window.innerWidth + 80,
      screenY: 20,
    });
    fireEvent.pointerUp(window, { pointerId: 1 });
  }

  it('reports the point to main on the way out, then commits on release', async () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dragTabOutAndRelease(container, 't1');

    expect(electron.tabDragMove).toHaveBeenCalled();
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    const request = electron.tabDragCommit.mock.calls[0][0];
    // The window-local identity does not travel; the session does.
    expect(request.tab).toEqual({ sessionId: 'sess-1', title: 'Chat 1', userSetName: false });
    expect(request.point.screenX).toBe(window.innerWidth + 80);
  });

  it('drops the tab only when main says the move HAPPENED', async () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dragTabOutAndRelease(container, 't1');
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    await waitFor(() => expect(dispatch).toHaveBeenCalledWith({ type: 'closeTab', tabId: 't1' }));
  });

  it('KEEPS the tab on `noop` — the answer to every way this can fail', async () => {
    // A target that refused the insert, a window that closed mid-gesture, a drop
    // that resolved to nothing. None of them may cost the user a chat.
    electron.tabDragCommit.mockResolvedValue({ outcome: 'noop' });
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dispatch.mockClear();
    dragTabOutAndRelease(container, 't1');
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    expect(dispatch.mock.calls.some(([action]) => action.type === 'closeTab')).toBe(false);
  });

  it('keeps the tab when the IPC itself fails', async () => {
    electron.tabDragCommit.mockRejectedValue(new Error('the daemon fell over'));
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dispatch.mockClear();
    dragTabOutAndRelease(container, 't1');
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    expect(dispatch.mock.calls.some(([action]) => action.type === 'closeTab')).toBe(false);
  });

  it('flags a window with more than one tab as NOT its last (D5 does not apply)', async () => {
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dragTabOutAndRelease(container, 't1');
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    expect(electron.tabDragCommit.mock.calls[0][0].isOnlyTab).toBe(false);
  });

  it('flags a single-tab window, so main can refuse the tear-off and allow the merge', async () => {
    // ⚠ In the real app this press never reaches React: with one tab,
    // `-webkit-app-region: drag` claims it and the OS moves the WINDOW (measured
    // both ways in Phase 0). This is the backstop for a platform without app
    // regions — and the flag main needs, because a tear-off of the last tab is a
    // no-op while a MERGE of it is the gesture and closes this window.
    state = stateWith(['solo']);
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dragTabOutAndRelease(container, 'solo');
    await waitFor(() => expect(electron.tabDragCommit).toHaveBeenCalled());
    expect(electron.tabDragCommit.mock.calls[0][0].isOnlyTab).toBe(true);
  });

  it('closes this window when a MERGE took its last tab (D6a)', async () => {
    state = stateWith(['solo']);
    electron.tabDragCommit.mockResolvedValue({ outcome: 'merge' });
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dispatch.mockClear();
    dragTabOutAndRelease(container, 'solo');

    await waitFor(() => expect(electron.closeWindow).toHaveBeenCalled());
    // Closing outright rather than closing the tab first: `closeTab` on the last
    // tab would bounce this window Home for the frame before it disappears.
    expect(dispatch.mock.calls.some(([action]) => action.type === 'closeTab')).toBe(false);
  });

  it('does NOT close the window when the merge left other tabs behind', async () => {
    electron.tabDragCommit.mockResolvedValue({ outcome: 'merge' });
    const { container } = render(<ChatGroupsShell onChatChange={() => {}} />);
    dragTabOutAndRelease(container, 't1');
    await waitFor(() => expect(dispatch).toHaveBeenCalledWith({ type: 'closeTab', tabId: 't1' }));
    expect(electron.closeWindow).not.toHaveBeenCalled();
  });
});

describe('ChatGroupsShell — the ghost', () => {
  it('renders no ghost until a drag is promoted', () => {
    const { queryByTestId } = render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(queryByTestId('chat-tab-ghost')).toBeNull();
  });
});

describe('tab payload — what may cross a window boundary', () => {
  it('carries the session and its name, and leaves the window-local identity behind', () => {
    // Restated at the shell because this is the layer that CALLS it: a `tabId`
    // that travelled would collide with a tab the receiving window already has,
    // and a `pendingInitialMessage` that travelled would re-send the message that
    // created the session.
    const payload = payloadFromTab({
      tabId: 't1',
      sessionId: 'sess-1',
      title: 'Chat 1',
      userSetName: true,
      pendingInitialMessage: 'do not re-send me',
      cwd: '/w',
    });
    expect(payload).toEqual({ sessionId: 'sess-1', title: 'Chat 1', userSetName: true, cwd: '/w' });
    expect('tabId' in payload).toBe(false);
    expect('pendingInitialMessage' in payload).toBe(false);
  });
});
