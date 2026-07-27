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
 *  which is where xterm parks focus in the real app. */
function focusInsideTerminal(): void {
  const dock = document.createElement('section');
  dock.setAttribute('data-testid', 'in-app-terminal-dock');
  const xtermTextarea = document.createElement('textarea');
  dock.appendChild(xtermTextarea);
  document.body.appendChild(dock);
  xtermTextarea.focus();
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
    focusInsideTerminal();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane);
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
    focusInsideTerminal();
    const closePane = vi.fn(() => true);
    registerCloseTerminalPane(closePane);
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
    focusInsideTerminal();
    registerCloseTerminalPane(() => false);
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

  it('walks the full ladder: pane, pane, then chat tab, then window', () => {
    // The browser/terminal-emulator ladder from the issue, end to end: two
    // panes go first, the dock dies with the second, focus falls back to the
    // chat, the tab goes next, and only then does Cmd+W mean "close window".
    focusInsideTerminal();
    let panes = 2;
    const dispose = registerCloseTerminalPane(() => {
      if (panes === 0) return false;
      panes -= 1;
      return true;
    });
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
