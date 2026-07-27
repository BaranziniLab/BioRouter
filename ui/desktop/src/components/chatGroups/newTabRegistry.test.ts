import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  registerNewTab,
  requestNewTab,
  consumePendingNewTab,
  hasPendingNewTab,
  acknowledgeNewTabCommit,
  resetNewTabRegistry,
} from './newTabRegistry';

describe('newTabRegistry — the Cmd+T hand-off', () => {
  beforeEach(resetNewTabRegistry);

  it('delegates to the registered handler and claims the keystroke', () => {
    const handler = vi.fn();
    registerNewTab(handler);

    expect(requestNewTab()).toBe(true);
    expect(handler).toHaveBeenCalledTimes(1);
    // The provider commits the dispatched tab and acknowledges; nothing is
    // left to cash in later.
    acknowledgeNewTabCommit(1);
    expect(consumePendingNewTab()).toBe(false);
  });

  it('with no handler it REMEMBERS the request instead of dropping it', () => {
    // Cmd+T on Settings: the root navigates to /pair, and the provider that
    // mounts there must still open a tab. Forgetting here would silently
    // downgrade Cmd+T to a plain navigation.
    expect(requestNewTab()).toBe(false);
    expect(consumePendingNewTab()).toBe(true);
  });

  it('a pending request is consumed exactly ONCE', () => {
    // StrictMode mounts, unmounts and remounts the provider, so the consuming
    // effect runs twice. A readable flag would open two blank tabs for one key.
    requestNewTab();
    expect(consumePendingNewTab()).toBe(true);
    expect(consumePendingNewTab()).toBe(false);
  });

  it('reports no pending request when nothing asked', () => {
    expect(consumePendingNewTab()).toBe(false);
  });

  it('disposing unregisters, and a later request falls back to pending', () => {
    const dispose = registerNewTab(vi.fn());
    dispose();

    expect(requestNewTab()).toBe(false);
    expect(consumePendingNewTab()).toBe(true);
  });

  describe('hasPendingNewTab — the non-consuming peek (issue #38)', () => {
    it('peeking does NOT consume: the provider still cashes the request in', () => {
      requestNewTab(); // no handler -> remembered
      expect(hasPendingNewTab()).toBe(true);
      expect(hasPendingNewTab()).toBe(true); // still there — peek is idempotent
      expect(consumePendingNewTab()).toBe(true); // the consume still wins the race
      expect(hasPendingNewTab()).toBe(false);
    });

    it('reports false when nothing is pending', () => {
      expect(hasPendingNewTab()).toBe(false);
    });

    it('a DISPATCHED request stays observable until the provider commits nonzero tabs', () => {
      // Codex review B6 finding 2: Cmd+T on a mounted /pair dispatches through
      // the handler, but the tab exists only after React commits the reducer
      // update. The zero-tab redirect effect can run in that gap and must
      // still see the request, or the keystroke is bounced Home.
      registerNewTab(vi.fn());
      requestNewTab();
      expect(hasPendingNewTab()).toBe(true);

      // A zero-tab commit (e.g. the stale close that preceded the Cmd+T) does
      // NOT retire it…
      acknowledgeNewTabCommit(0);
      expect(hasPendingNewTab()).toBe(true);

      // …the commit that actually lands the tab does.
      acknowledgeNewTabCommit(1);
      expect(hasPendingNewTab()).toBe(false);
    });

    it('a dispatch that died with an unmounting provider is honoured by the next mount', () => {
      // The dispatch was queued into a reducer that unmounted before
      // committing: never acknowledged. The next provider's consume-once must
      // treat it as a remembered request — the user pressed Cmd+T.
      registerNewTab(vi.fn());
      requestNewTab();
      expect(consumePendingNewTab()).toBe(true);
      expect(consumePendingNewTab()).toBe(false); // still consume-once
    });

    it('consume still clears exactly once after any number of peeks', () => {
      requestNewTab();
      hasPendingNewTab();
      hasPendingNewTab();
      expect(consumePendingNewTab()).toBe(true);
      expect(consumePendingNewTab()).toBe(false);
    });
  });

  it('a StrictMode double-mount cannot leave the registry empty', () => {
    // Mount A, mount B, THEN dispose A — React's order. A's disposer must not
    // clear B's handler, or Cmd+T would navigate instead of opening a tab.
    const a = vi.fn();
    const b = vi.fn();
    const disposeA = registerNewTab(a);
    registerNewTab(b);
    disposeA();

    expect(requestNewTab()).toBe(true);
    expect(b).toHaveBeenCalledTimes(1);
    expect(a).not.toHaveBeenCalled();
  });
});
