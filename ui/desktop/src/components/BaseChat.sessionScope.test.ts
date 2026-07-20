import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  artifactPanelTargetContentWidth,
  createScrollToBottomHandler,
  createSessionDivergedHandler,
  isEventForSession,
  SCROLL_TO_BOTTOM_DELAY_MS,
  submitAndReturnToBottom,
} from './BaseChat';

/**
 * These events are BROADCAST on `window`, so every mounted BaseChat hears every
 * one of them. That is latent on /pair today (one BaseChat) but tabbed chat
 * mounts N of them on /pair.
 *
 * Mounting two real BaseChats here is not practical — BaseChatContent is a
 * ~1900-line component that needs react-router plus useConfig / useSidebar /
 * useDiverge / useNavigation / useFileDrop / useChatStream / useCostTracking /
 * useToolCount / ChatContext / window.electron / window.appConfig, and renders
 * the full message tree and every modal. A test built on that much mock
 * scaffolding would mostly be asserting against the scaffolding.
 *
 * Instead we register TWO REAL listeners — built by the same exported factories
 * the component's effects call, so the guard under test cannot drift from the
 * guard that ships — on the REAL window, and dispatch REAL CustomEvents. That
 * exercises the exact mechanism the bug lives in: N listeners, one broadcast.
 * The only thing left uncovered is that each effect passes its own `sessionId`
 * into the factory, which is one visible line per effect and typechecked.
 */

const SESSION_A = 'session-aaa';
const SESSION_B = 'session-bbb';

describe('broadcast window events are scoped to their own chat', () => {
  // Stands in for two mounted BaseChats: same events, different sessionIds.
  type ScrollFn = () => void;
  type NavigateFn = (to: string, options: { state: Record<string, unknown> }) => void;

  let scrollA: ReturnType<typeof vi.fn<ScrollFn>>;
  let scrollB: ReturnType<typeof vi.fn<ScrollFn>>;
  let navigateA: ReturnType<typeof vi.fn<NavigateFn>>;
  let navigateB: ReturnType<typeof vi.fn<NavigateFn>>;
  let listeners: Array<[string, globalThis.EventListener]>;

  const mountTwoChats = () => {
    const handlers: Array<[string, globalThis.EventListener]> = [
      ['scroll-chat-to-bottom', createScrollToBottomHandler({ sessionId: SESSION_A, scrollToBottom: scrollA })],
      ['scroll-chat-to-bottom', createScrollToBottomHandler({ sessionId: SESSION_B, scrollToBottom: scrollB })],
      ['session-diverged', createSessionDivergedHandler({ sessionId: SESSION_A, navigate: navigateA })],
      ['session-diverged', createSessionDivergedHandler({ sessionId: SESSION_B, navigate: navigateB })],
    ];
    handlers.forEach(([type, fn]) => window.addEventListener(type, fn));
    listeners = handlers;
  };

  beforeEach(() => {
    vi.useFakeTimers();
    scrollA = vi.fn<ScrollFn>();
    scrollB = vi.fn<ScrollFn>();
    navigateA = vi.fn<NavigateFn>();
    navigateB = vi.fn<NavigateFn>();
    mountTwoChats();
  });

  afterEach(() => {
    listeners.forEach(([type, fn]) => window.removeEventListener(type, fn));
    vi.useRealTimers();
  });

  describe('session-diverged', () => {
    it('navigates ONLY the chat the user diverged from, not every mounted chat', () => {
      window.dispatchEvent(
        new CustomEvent('session-diverged', {
          detail: {
            sessionId: SESSION_A, // origin: the chat the user diverged FROM
            newSessionId: 'session-new',
            shouldStartAgent: true,
            editedMessage: 'rerun with n=20',
          },
        })
      );

      expect(navigateA).toHaveBeenCalledTimes(1);
      // The bug: B navigates too, so N chats race to the same URL.
      expect(navigateB).not.toHaveBeenCalled();
    });

    it('preserves the full navigation target for the chat that does act', () => {
      window.dispatchEvent(
        new CustomEvent('session-diverged', {
          detail: {
            sessionId: SESSION_A,
            newSessionId: 'session-new',
            shouldStartAgent: true,
            editedMessage: 'rerun with n=20',
          },
        })
      );

      expect(navigateA).toHaveBeenCalledWith('/pair?resumeSessionId=session-new&shouldStartAgent=true', {
        state: { disableAnimation: true, initialMessage: 'rerun with n=20' },
      });
    });

    it('omits shouldStartAgent from the query when it is not set', () => {
      window.dispatchEvent(
        new CustomEvent('session-diverged', {
          detail: { sessionId: SESSION_B, newSessionId: 'session-new' },
        })
      );

      expect(navigateA).not.toHaveBeenCalled();
      expect(navigateB).toHaveBeenCalledWith('/pair?resumeSessionId=session-new', {
        state: { disableAnimation: true, initialMessage: undefined },
      });
    });

    it('matches on the ORIGIN session, not on newSessionId', () => {
      // newSessionId names a session that does not exist in the UI yet, so a
      // listener keyed on it would match nobody and the divergence would
      // silently never navigate.
      window.dispatchEvent(
        new CustomEvent('session-diverged', {
          detail: { sessionId: SESSION_A, newSessionId: SESSION_B },
        })
      );

      expect(navigateA).toHaveBeenCalledTimes(1);
      // B must NOT act just because the new session happens to carry its id.
      expect(navigateB).not.toHaveBeenCalled();
    });
  });

  describe('scroll-chat-to-bottom', () => {
    it('scrolls ONLY the chat the artifact is rendered inside', () => {
      window.dispatchEvent(
        new CustomEvent('scroll-chat-to-bottom', { detail: { sessionId: SESSION_A } })
      );
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(scrollA).toHaveBeenCalledTimes(1);
      // The bug: an MCP-UI prompt action in chat A scrolls chat B.
      expect(scrollB).not.toHaveBeenCalled();
    });

    it('does not scroll before the render delay has elapsed', () => {
      window.dispatchEvent(
        new CustomEvent('scroll-chat-to-bottom', { detail: { sessionId: SESSION_A } })
      );
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS - 1);
      expect(scrollA).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      expect(scrollA).toHaveBeenCalledTimes(1);
    });
  });

  /**
   * Issue 3 — `handleRenderingComplete` force-scrolls only on the FIRST render
   * and otherwise scrolls only `if (scrollRef.current?.isFollowing)`. Scrolling
   * up clears isFollowing (ui/scroll-area.tsx sets userScrolledUpRef and
   * setIsFollowing(false)), so a user reading history who then sent a message
   * was left stranded mid-transcript, with no sight of where their prompt
   * landed.
   *
   * These register the REAL listener the component's effect builds, so the fix
   * is exercised through the same mechanism that ships.
   */
  describe('a user submit returns that chat to the bottom', () => {
    it('scrolls the submitting chat even when it had been scrolled up', () => {
      const submit = vi.fn();
      // isFollowing is irrelevant here BY DESIGN: this path does not consult
      // it. That is the fix — the previous behaviour gated on it and so did
      // nothing for a user who had scrolled up.
      submitAndReturnToBottom({ sessionId: SESSION_A, submit }, 'my next question');
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(submit).toHaveBeenCalledWith('my next question', undefined);
      expect(scrollA).toHaveBeenCalledTimes(1);
    });

    it('does not drag a DIFFERENT chat to the bottom', () => {
      submitAndReturnToBottom({ sessionId: SESSION_A, submit: vi.fn() }, 'hello');
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(scrollA).toHaveBeenCalledTimes(1);
      expect(scrollB).not.toHaveBeenCalled();
    });

    it('does not broadcast to every chat when the submitting chat has no session yet', () => {
      // A chat still creating its session submits with sessionId ''. That is
      // FALSY, so `isEventForSession` would read it as an un-addressed
      // broadcast (a deliberate back-compat branch) and BOTH chats would obey
      // it — yanking a background pane the user was reading. The sender drops
      // it instead: a chat with no session has no transcript to scroll.
      const submit = vi.fn();
      submitAndReturnToBottom({ sessionId: '', submit }, 'sent while still creating');
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(submit).toHaveBeenCalledWith('sent while still creating', undefined);
      expect(scrollA).not.toHaveBeenCalled();
      expect(scrollB).not.toHaveBeenCalled();
    });

    it('forwards attachments untouched', () => {
      const submit = vi.fn();
      const attachments = [{ path: '/tmp/plot.png', kind: 'image' as const }];
      submitAndReturnToBottom({ sessionId: SESSION_A, submit }, 'look at this', attachments);

      expect(submit).toHaveBeenCalledWith('look at this', attachments);
    });

    it('submits BEFORE asking to scroll, so the new message is on screen to scroll to', () => {
      const order: string[] = [];
      const submit = vi.fn(() => void order.push('submit'));
      window.addEventListener('scroll-chat-to-bottom', () => order.push('scroll-request'));

      submitAndReturnToBottom({ sessionId: SESSION_A, submit }, 'ordering');

      expect(order).toEqual(['submit', 'scroll-request']);
    });

    it('scrolls smoothly — the instant jump is reserved for the first render', () => {
      // The handler calls scrollToBottom() with no argument; ScrollArea defaults
      // that to 'smooth' and only handleRenderingComplete's resume path passes
      // 'auto'. Landing a submit instantly would be a jarring teleport.
      submitAndReturnToBottom({ sessionId: SESSION_A, submit: vi.fn() }, 'smooth please');
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(scrollA).toHaveBeenCalledWith();
    });
  });

  describe('back-compat: an un-scoped broadcast still reaches everyone', () => {
    // The filter is deliberately lenient (the ChatInput.tsx:379-382 idiom): an
    // event with no sessionId is a true broadcast. Every in-app dispatcher now
    // sets one, so this only serves a dispatcher we don't control.
    it('scrolls every chat when the event carries no sessionId', () => {
      window.dispatchEvent(new CustomEvent('scroll-chat-to-bottom'));
      vi.advanceTimersByTime(SCROLL_TO_BOTTOM_DELAY_MS);

      expect(scrollA).toHaveBeenCalledTimes(1);
      expect(scrollB).toHaveBeenCalledTimes(1);
    });
  });
});

describe('artifactPanelTargetContentWidth', () => {
  // A window resize is app-scoped but BaseChat is session-scoped: a BACKGROUND
  // chat opening an artifact must never resize the OS window.
  const wide = { windowWidth: 1200, splitPaneWidth: 800 };

  it('returns a target width for a focused desktop chat', () => {
    const width = artifactPanelTargetContentWidth({
      isMobile: false,
      allowWindowResize: true,
      ...wide,
    });
    expect(width).toBeGreaterThan(0);
  });

  it('refuses to resize when the chat is not allowed to (a background group)', () => {
    expect(
      artifactPanelTargetContentWidth({ isMobile: false, allowWindowResize: false, ...wide })
    ).toBeNull();
  });

  it('refuses to resize on mobile even when allowed', () => {
    expect(
      artifactPanelTargetContentWidth({ isMobile: true, allowWindowResize: true, ...wide })
    ).toBeNull();
  });
});

describe('isEventForSession', () => {
  it('accepts an event addressed to this session', () => {
    expect(isEventForSession({ sessionId: SESSION_A }, SESSION_A)).toBe(true);
  });

  it('rejects an event addressed to a different session', () => {
    expect(isEventForSession({ sessionId: SESSION_B }, SESSION_A)).toBe(false);
  });

  it.each([
    ['undefined detail', undefined],
    ['null detail', null],
    ['detail without a sessionId', {}],
    ['an explicitly null sessionId', { sessionId: null }],
  ])('treats %s as an un-addressed broadcast', (_label, detail) => {
    expect(isEventForSession(detail, SESSION_A)).toBe(true);
  });
});
