import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isTerminalFocused,
  registerNewTerminalPane,
  requestNewTerminalPane,
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
