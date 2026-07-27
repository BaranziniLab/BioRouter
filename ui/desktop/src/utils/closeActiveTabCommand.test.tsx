import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { runCloseActiveTabCommand } from './closeActiveTabCommand';
import {
  registerCloseTerminalPane,
  resetCloseTerminalPaneRegistry,
  resetNewTerminalPaneRegistry,
} from './terminalFocus';
import {
  registerCloseActiveTab,
  resetCloseActiveTabRegistry,
} from '../components/chatGroups/closeActiveTabRegistry';

/**
 * REGRESSION GATE — issue #21, the Cmd+W half.
 *
 * "Cmd+W closes the entire app window instead of the focused terminal pane."
 * The keystroke NEVER reaches the DOM: main.ts's File > Close Tab menu item
 * owns CmdOrCtrl+W and delivers IPC, which App.tsx answers with
 * runCloseActiveTabCommand. So — exactly as keyboardResubmitGuard.test.tsx
 * establishes for Cmd+T/Cmd+W on chat tabs — the registries are the keyboard's
 * entry point, and driving the command function IS driving the keystroke.
 *
 * The ladder under test: focused terminal pane → chat tab → window.
 */

/** A stand-in dock: focus lands on a child of [data-testid=in-app-terminal-dock],
 *  which is where xterm parks focus in the real app. Returns the dock root —
 *  registrations are per dock and carry it, exactly as InAppTerminalDock does. */
function focusInsideTerminal(): HTMLElement {
  const dock = document.createElement('section');
  dock.setAttribute('data-testid', 'in-app-terminal-dock');
  const xtermTextarea = document.createElement('textarea');
  dock.appendChild(xtermTextarea);
  document.body.appendChild(dock);
  xtermTextarea.focus();
  return dock;
}

function focusComposer(): void {
  const composer = document.createElement('textarea');
  document.body.appendChild(composer);
  composer.focus();
}

describe('runCloseActiveTabCommand — the Cmd+W ladder (issue #21)', () => {
  const closeWindow = vi.fn();

  beforeEach(() => {
    closeWindow.mockClear();
    resetCloseTerminalPaneRegistry();
    resetNewTerminalPaneRegistry();
    resetCloseActiveTabRegistry();
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('terminal focused + dock open: closes the PANE — never the chat tab, never the window', () => {
    const dock = focusInsideTerminal();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane, () => dock);
    const closeChatTab = vi.fn(() => true);
    registerCloseActiveTab(closeChatTab);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    expect(closePane).toHaveBeenCalledTimes(1);
    expect(closeChatTab).not.toHaveBeenCalled();
    expect(closeWindow).not.toHaveBeenCalled();
  });

  it('REGRESSION (#21): terminal focused with ZERO chat tabs closes the pane, not the window', () => {
    // The dangerous path the issue reports: on the tabless surface the chat
    // registry claims nothing, and pre-fix Cmd+W fell through to closeWindow
    // even though the user was typing in a terminal.
    const dock = focusInsideTerminal();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane, () => dock);
    registerCloseActiveTab(() => false); // no active tab to close

    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    expect(closePane).toHaveBeenCalledTimes(1);
    expect(closeWindow).not.toHaveBeenCalled();
  });

  it('terminal focused but no dock registered (stale focus): falls through to the chat tab', () => {
    focusInsideTerminal();
    const closeChatTab = vi.fn(() => true);
    registerCloseActiveTab(closeChatTab);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('chat-tab');
    expect(closeChatTab).toHaveBeenCalledTimes(1);
    expect(closeWindow).not.toHaveBeenCalled();
  });

  it('terminal focused, dock declines (no active pane): falls through to the chat tab', () => {
    const dock = focusInsideTerminal();
    registerCloseTerminalPane(() => false, () => dock);
    const closeChatTab = vi.fn(() => true);
    registerCloseActiveTab(closeChatTab);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('chat-tab');
    expect(closeChatTab).toHaveBeenCalledTimes(1);
  });

  it('composer focused: the terminal rung is skipped even while a dock is open', () => {
    focusComposer();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane);
    const closeChatTab = vi.fn(() => true);
    registerCloseActiveTab(closeChatTab);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('chat-tab');
    expect(closePane).not.toHaveBeenCalled();
    expect(closeChatTab).toHaveBeenCalledTimes(1);
  });

  it('nothing claims the keystroke: the window closes (Settings, tabless routes)', () => {
    focusComposer();

    expect(runCloseActiveTabCommand(closeWindow)).toBe('window');
    expect(closeWindow).toHaveBeenCalledTimes(1);
  });

  it('REGRESSION (Codex B6.4): zero chat tabs + live dock + focus LOST closes the pane, not the window', () => {
    // The gated zero-tab /pair with a terminal open on the empty pane: the
    // user clicked out of the terminal (activeElement moved to the surface),
    // so rung 1 stands down and no chat tab exists for rung 2. Pre-fix the
    // window closed out from under a live pane; the last-chance rung must let
    // the visible dock claim the keystroke.
    const dock = document.createElement('section');
    dock.setAttribute('data-testid', 'in-app-terminal-dock');
    document.body.appendChild(dock);
    focusComposer(); // focus is NOT in the dock
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane, () => dock);
    registerCloseActiveTab(() => false); // zero tabs

    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    expect(closePane).toHaveBeenCalledTimes(1);
    expect(closeWindow).not.toHaveBeenCalled();
  });

  it('last-chance rung with TWO visible docks: the last-focused dock claims the keystroke', () => {
    // Docks register when they OPEN; the user focuses one afterwards — the
    // registry's focus tracking only exists while docks are registered.
    const dockA = document.createElement('section');
    dockA.setAttribute('data-testid', 'in-app-terminal-dock');
    const xtermA = document.createElement('textarea');
    dockA.appendChild(xtermA);
    document.body.appendChild(dockA);
    const dockB = document.createElement('section');
    dockB.setAttribute('data-testid', 'in-app-terminal-dock');
    document.body.appendChild(dockB);
    const closePaneA = vi.fn(() => true);
    const closePaneB = vi.fn(() => true);
    registerCloseTerminalPane(closePaneA, () => dockA);
    registerCloseTerminalPane(closePaneB, () => dockB); // registered last
    xtermA.focus(); // the user last typed in dock A…
    focusComposer(); // …then clicked into the chat surface
    registerCloseActiveTab(() => false); // zero tabs

    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    expect(closePaneA).toHaveBeenCalledTimes(1);
    expect(closePaneB).not.toHaveBeenCalled();
    expect(closeWindow).not.toHaveBeenCalled();
  });

  it('the last-chance rung never outranks a chat tab: composer focus + tabs still closes the tab', () => {
    const dock = document.createElement('section');
    dock.setAttribute('data-testid', 'in-app-terminal-dock');
    document.body.appendChild(dock);
    focusComposer();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane, () => dock);
    const closeChatTab = vi.fn(() => true);
    registerCloseActiveTab(closeChatTab);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('chat-tab');
    expect(closePane).not.toHaveBeenCalled();
    expect(closeChatTab).toHaveBeenCalledTimes(1);
  });

  it('a declining dock cannot hold the window open: everything declines → window closes', () => {
    const dock = document.createElement('section');
    dock.setAttribute('data-testid', 'in-app-terminal-dock');
    document.body.appendChild(dock);
    focusComposer();
    registerCloseTerminalPane(() => false, () => dock); // no active pane
    registerCloseActiveTab(() => false);

    expect(runCloseActiveTabCommand(closeWindow)).toBe('window');
    expect(closeWindow).toHaveBeenCalledTimes(1);
  });

  it('walks the full ladder: pane, pane, then chat tab, then window', () => {
    // The browser/terminal-emulator ladder from the issue, end to end: two
    // panes go first, the dock dies with the second, focus falls back to the
    // chat, the tab goes next, and only then does Cmd+W mean "close window".
    const dock = focusInsideTerminal();
    let panes = 2;
    const dispose = registerCloseTerminalPane(() => {
      if (panes === 0) return false;
      panes -= 1;
      return true;
    }, () => dock);
    let tabs = 1;
    registerCloseActiveTab(() => {
      if (tabs === 0) return false;
      tabs -= 1;
      return true;
    });

    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    expect(runCloseActiveTabCommand(closeWindow)).toBe('terminal-pane');
    // Last pane closed -> the dock unmounts: its registration disposes and
    // focus leaves the terminal.
    dispose();
    (document.activeElement as HTMLElement | null)?.blur();
    expect(runCloseActiveTabCommand(closeWindow)).toBe('chat-tab');
    expect(closeWindow).not.toHaveBeenCalled();
    expect(runCloseActiveTabCommand(closeWindow)).toBe('window');
    expect(closeWindow).toHaveBeenCalledTimes(1);
  });
});
