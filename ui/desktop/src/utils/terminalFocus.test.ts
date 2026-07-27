import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isTerminalFocused,
  registerCloseTerminalPane,
  registerNewTerminalPane,
  requestCloseTerminalPane,
  requestNewTerminalPane,
  resetCloseTerminalPaneRegistry,
  resetNewTerminalPaneRegistry,
} from './terminalFocus';

describe('isTerminalFocused', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  function dock(): HTMLElement {
    const el = document.createElement('section');
    el.setAttribute('data-testid', 'in-app-terminal-dock');
    document.body.appendChild(el);
    return el;
  }

  it('is true when the active element is inside the terminal dock', () => {
    const el = dock();
    const inner = document.createElement('textarea'); // xterm's focus target
    el.appendChild(inner);
    expect(isTerminalFocused(inner)).toBe(true);
    // The dock itself counts too — closest matches self.
    expect(isTerminalFocused(el)).toBe(true);
  });

  it('is false when focus is outside any terminal dock', () => {
    dock();
    const composer = document.createElement('textarea');
    document.body.appendChild(composer);
    expect(isTerminalFocused(composer)).toBe(false);
  });

  it('is false for a null/non-element active element (e.g. nothing focused)', () => {
    expect(isTerminalFocused(null)).toBe(false);
  });

  it('reads document.activeElement by default', () => {
    const el = dock();
    const inner = document.createElement('textarea');
    el.appendChild(inner);
    inner.focus();
    expect(isTerminalFocused()).toBe(true);
    inner.blur();
    expect(isTerminalFocused()).toBe(false);
  });
});

describe('new-terminal-pane registry', () => {
  beforeEach(() => resetNewTerminalPaneRegistry());
  afterEach(() => resetNewTerminalPaneRegistry());

  it('returns false when no terminal has registered (fall through to a chat tab)', () => {
    expect(requestNewTerminalPane()).toBe(false);
  });

  it('calls the registered handler and reports true', () => {
    const addPane = vi.fn();
    registerNewTerminalPane(addPane);
    expect(requestNewTerminalPane()).toBe(true);
    expect(addPane).toHaveBeenCalledTimes(1);
  });

  it('last registration wins — the visible dock owns the gesture', () => {
    const first = vi.fn();
    const second = vi.fn();
    registerNewTerminalPane(first);
    registerNewTerminalPane(second);
    requestNewTerminalPane();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('disposing clears only if still the installed handler (StrictMode-safe)', () => {
    const a = vi.fn();
    const b = vi.fn();
    const disposeA = registerNewTerminalPane(a);
    registerNewTerminalPane(b); // B mounts before A disposes
    disposeA(); // must NOT clear B
    expect(requestNewTerminalPane()).toBe(true);
    expect(b).toHaveBeenCalledTimes(1);
    expect(a).not.toHaveBeenCalled();
  });

  it('disposing the current handler empties the registry', () => {
    const a = vi.fn();
    const disposeA = registerNewTerminalPane(a);
    disposeA();
    expect(requestNewTerminalPane()).toBe(false);
  });
});

describe('close-terminal-pane registry (issue #21 — the Cmd+W mirror)', () => {
  beforeEach(() => resetCloseTerminalPaneRegistry());
  afterEach(() => resetCloseTerminalPaneRegistry());

  it('returns false when no terminal has registered (fall through to the chat tab)', () => {
    expect(requestCloseTerminalPane()).toBe(false);
  });

  it('calls the registered handler and reports its claim', () => {
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane);
    expect(requestCloseTerminalPane()).toBe(true);
    expect(closePane).toHaveBeenCalledTimes(1);
  });

  it('a handler may DECLINE (no active pane) and the request falls through', () => {
    registerCloseTerminalPane(() => false);
    expect(requestCloseTerminalPane()).toBe(false);
  });

  it('last registration wins — the visible dock owns the gesture', () => {
    const first = vi.fn(() => true);
    const second = vi.fn(() => true);
    registerCloseTerminalPane(first);
    registerCloseTerminalPane(second);
    requestCloseTerminalPane();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('disposing clears only if still the installed handler (StrictMode-safe)', () => {
    const a = vi.fn(() => true);
    const b = vi.fn(() => true);
    const disposeA = registerCloseTerminalPane(a);
    registerCloseTerminalPane(b); // B mounts before A disposes
    disposeA(); // must NOT clear B
    expect(requestCloseTerminalPane()).toBe(true);
    expect(b).toHaveBeenCalledTimes(1);
    expect(a).not.toHaveBeenCalled();
  });

  it('disposing the current handler empties the registry', () => {
    const a = vi.fn(() => true);
    const disposeA = registerCloseTerminalPane(a);
    disposeA();
    expect(requestCloseTerminalPane()).toBe(false);
  });

  it('is independent of the new-pane registry — one never answers for the other', () => {
    registerNewTerminalPane(vi.fn());
    expect(requestCloseTerminalPane()).toBe(false);
    resetNewTerminalPaneRegistry();
    registerCloseTerminalPane(vi.fn(() => true));
    expect(requestNewTerminalPane()).toBe(false);
  });
});

// Terminals are PER-PANE: a split renders one dock per pane and several can be
// open (registered) at once — see ChatGroupsShell. A single last-write-wins
// slot routed Cmd+W to whichever dock rendered last; registrations now carry
// the dock root and requests route by focus (Codex review B6 finding 3).
describe('multi-dock routing (split layouts)', () => {
  beforeEach(() => {
    resetNewTerminalPaneRegistry();
    resetCloseTerminalPaneRegistry();
  });
  afterEach(() => {
    resetNewTerminalPaneRegistry();
    resetCloseTerminalPaneRegistry();
    document.body.innerHTML = '';
  });

  function dockWithInput(): { dock: HTMLElement; input: HTMLTextAreaElement } {
    const dock = document.createElement('section');
    dock.setAttribute('data-testid', 'in-app-terminal-dock');
    const input = document.createElement('textarea'); // xterm's focus target
    dock.appendChild(input);
    document.body.appendChild(dock);
    return { dock, input };
  }

  it('Cmd+W routes to the dock HOLDING FOCUS, not the last-registered one', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const closeA = vi.fn(() => true);
    const closeB = vi.fn(() => true);
    registerCloseTerminalPane(closeA, () => a.dock);
    registerCloseTerminalPane(closeB, () => b.dock); // registered LAST
    a.input.focus();

    expect(requestCloseTerminalPane()).toBe(true);
    expect(closeA).toHaveBeenCalledTimes(1);
    expect(closeB).not.toHaveBeenCalled();
  });

  it('Cmd+T adds a pane to the focused dock, not the last-registered one', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const addA = vi.fn();
    const addB = vi.fn();
    registerNewTerminalPane(addA, () => a.dock);
    registerNewTerminalPane(addB, () => b.dock);
    a.input.focus();

    expect(requestNewTerminalPane()).toBe(true);
    expect(addA).toHaveBeenCalledTimes(1);
    expect(addB).not.toHaveBeenCalled();
  });

  it('a focused dock answers ALONE: its decline falls through, never to a sibling', () => {
    // The user means the terminal under their cursor; reaching into another
    // pane's dock on a decline would close a pane they are not looking at.
    const a = dockWithInput();
    const b = dockWithInput();
    const closeB = vi.fn(() => true);
    registerCloseTerminalPane(
      () => false,
      () => a.dock
    );
    registerCloseTerminalPane(closeB, () => b.dock);
    a.input.focus();

    expect(requestCloseTerminalPane()).toBe(false);
    expect(closeB).not.toHaveBeenCalled();
  });

  it('disposing one dock leaves the OTHER dock registered and reachable', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const closeA = vi.fn(() => true);
    const closeB = vi.fn(() => true);
    const disposeA = registerCloseTerminalPane(closeA, () => a.dock);
    registerCloseTerminalPane(closeB, () => b.dock);
    disposeA();
    b.input.focus();

    expect(requestCloseTerminalPane()).toBe(true);
    expect(closeB).toHaveBeenCalledTimes(1);
    expect(closeA).not.toHaveBeenCalled();
  });

  it('focus LOST: the last-FOCUSED dock claims before the newest registration', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const closeA = vi.fn(() => true);
    const closeB = vi.fn(() => true);
    registerCloseTerminalPane(closeA, () => a.dock);
    registerCloseTerminalPane(closeB, () => b.dock); // newest
    a.input.focus(); // the user last typed in dock A…
    const composer = document.createElement('textarea');
    document.body.appendChild(composer);
    composer.focus(); // …then clicked into the chat

    expect(requestCloseTerminalPane()).toBe(true);
    expect(closeA).toHaveBeenCalledTimes(1);
    expect(closeB).not.toHaveBeenCalled();
  });

  it('focus lost and never in a dock: the newest registration answers', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const closeA = vi.fn(() => true);
    const closeB = vi.fn(() => true);
    registerCloseTerminalPane(closeA, () => a.dock);
    registerCloseTerminalPane(closeB, () => b.dock);

    expect(requestCloseTerminalPane()).toBe(true);
    expect(closeB).toHaveBeenCalledTimes(1);
    expect(closeA).not.toHaveBeenCalled();
  });

  it('a focused dock that never registered claims nothing (stale focus)', () => {
    // Only a hidden dock is unregistered, and display:none cannot hold focus —
    // but if focus IS somehow inside an unregistered dock, no other dock may
    // answer for it.
    const stale = dockWithInput();
    const registered = dockWithInput();
    const close = vi.fn(() => true);
    registerCloseTerminalPane(close, () => registered.dock);
    stale.input.focus();

    expect(requestCloseTerminalPane()).toBe(false);
    expect(close).not.toHaveBeenCalled();
  });
});

// Codex B6 re-review finding 3 — the commit-to-cleanup race. A dock's
// registration is disposed by a passive effect cleanup, which runs AFTER the
// commit that hid (`open` -> false paints the `hidden` class) or removed its
// DOM. A Cmd+W IPC landing in that window found the stale registration still
// in the registry, and the last-chance fallback let it close a pane in a dock
// the user could no longer see. The fallback now rechecks the root: connected
// AND visible, or the registration is not offered.
describe('commit-to-cleanup race — a hidden or unmounted dock cannot claim the fallback', () => {
  beforeEach(() => {
    resetNewTerminalPaneRegistry();
    resetCloseTerminalPaneRegistry();
  });
  afterEach(() => {
    resetNewTerminalPaneRegistry();
    resetCloseTerminalPaneRegistry();
    document.body.innerHTML = '';
  });

  function dockWithInput(): { dock: HTMLElement; input: HTMLTextAreaElement } {
    const dock = document.createElement('section');
    dock.setAttribute('data-testid', 'in-app-terminal-dock');
    const input = document.createElement('textarea');
    dock.appendChild(input);
    document.body.appendChild(dock);
    return { dock, input };
  }

  it('Cmd+W skips a still-registered dock whose root was HIDDEN this commit', () => {
    const a = dockWithInput();
    const close = vi.fn(() => true);
    registerCloseTerminalPane(close, () => a.dock);
    a.dock.style.display = 'none'; // committed; the unregistering cleanup has not run yet

    expect(requestCloseTerminalPane()).toBe(false); // falls through the Cmd+W ladder
    expect(close).not.toHaveBeenCalled();
  });

  it('Cmd+W skips a still-registered dock whose root was UNMOUNTED this commit', () => {
    const a = dockWithInput();
    const close = vi.fn(() => true);
    registerCloseTerminalPane(close, () => a.dock);
    a.dock.remove();

    expect(requestCloseTerminalPane()).toBe(false);
    expect(close).not.toHaveBeenCalled();
  });

  it('the next USABLE dock answers instead of the newest-but-hidden one', () => {
    const a = dockWithInput();
    const b = dockWithInput();
    const closeA = vi.fn(() => true);
    const closeB = vi.fn(() => true);
    registerCloseTerminalPane(closeA, () => a.dock);
    registerCloseTerminalPane(closeB, () => b.dock); // newest — would win the fallback
    b.dock.style.display = 'none';

    expect(requestCloseTerminalPane()).toBe(true);
    expect(closeA).toHaveBeenCalledTimes(1);
    expect(closeB).not.toHaveBeenCalled();
  });

  it('Cmd+T falls through to a chat tab when every registered dock is hidden', () => {
    const a = dockWithInput();
    const add = vi.fn();
    registerNewTerminalPane(add, () => a.dock);
    a.dock.style.display = 'none';

    expect(requestNewTerminalPane()).toBe(false);
    expect(add).not.toHaveBeenCalled();
  });

  it('a rootless (pure-registry) registration keeps its fallback semantics', () => {
    // Test registrations may omit getRoot; they can never match a focused dock
    // but must stay reachable through the ordering fallbacks.
    const close = vi.fn(() => true);
    registerCloseTerminalPane(close);

    expect(requestCloseTerminalPane()).toBe(true);
    expect(close).toHaveBeenCalledTimes(1);
  });
});
