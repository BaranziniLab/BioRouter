import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import InAppTerminalDock, { MAX_TERMINAL_PANES } from './InAppTerminalDock';
import {
  requestCloseTerminalPane,
  resetCloseTerminalPaneRegistry,
  resetNewTerminalPaneRegistry,
} from '../utils/terminalFocus';
import { resetTerminalRunChannelForTests, runInTerminal } from '../utils/terminalRunChannel';
import { GENERATED_THEMES } from '../styles/themes.generated';

interface FakeTerminal {
  modes: { bracketedPasteMode: boolean };
  focus: ReturnType<typeof vi.fn>;
}

/**
 * Every xterm instance the dock has constructed, newest last — one per pane.
 *
 * `modes` is xterm's live report of what the SHELL asked for (DECSET 2004), and
 * the Run path reads it to decide whether to bracket the paste. A double
 * without it would let the un-bracketed branch pass for every case, including
 * the multi-line one that exists to exercise the other branch.
 */
const xtermInstances = vi.hoisted(() => [] as FakeTerminal[]);

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    modes = { bracketedPasteMode: false };
    dispose = vi.fn();
    focus = vi.fn();
    loadAddon = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    open = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    constructor() {
      xtermInstances.push(this as unknown as FakeTerminal);
    }
  },
}));

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit = vi.fn();
    proposeDimensions = vi.fn(() => ({ cols: 80, rows: 18 }));
  },
}));

const terminalDisposer = vi.fn();

beforeEach(() => {
  terminalDisposer.mockClear();
  resetCloseTerminalPaneRegistry();
  resetNewTerminalPaneRegistry();
  resetTerminalRunChannelForTests();
  xtermInstances.length = 0;
  let nextSessionId = 0;
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      // Deterministic and ordered, so a test can say WHICH pane's shell a
      // command reached rather than only that some write happened.
      createTerminalSession: vi.fn(async () => ({
        backend: 'pty',
        cwd: '/Users/wgu/Desktop/biorouter',
        sessionId: `pty-${(nextSessionId += 1)}`,
        success: true,
      })),
      disposeTerminalSession: vi.fn(async () => ({ success: true })),
      onTerminalData: vi.fn(() => terminalDisposer),
      onTerminalExit: vi.fn(() => terminalDisposer),
      resizeTerminalSession: vi.fn(async () => ({ success: true })),
      writeTerminalSession: vi.fn(async () => ({ success: true })),
    },
  });
  Object.defineProperty(window, 'appConfig', {
    configurable: true,
    value: { get: vi.fn(() => '/Users/wgu') },
  });
  Object.defineProperty(window, 'ResizeObserver', {
    configurable: true,
    value: class {
      disconnect = vi.fn();
      observe = vi.fn();
    },
  });
});

describe('InAppTerminalDock', () => {
  it('opens with a visible active tab and lets users add another terminal tab', async () => {
    const user = userEvent.setup();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />);

    const tabList = screen.getByRole('tablist', { name: /^terminals$/i });
    expect(tabList).toBeInTheDocument();
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('hidden');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('invisible');

    await user.click(screen.getByRole('button', { name: /new terminal/i }));

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter 2');
  });

  it('closes only the selected terminal tab and keeps another tab active', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={onClose} />);

    await user.click(screen.getByRole('button', { name: /new terminal/i }));
    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });

    await user.click(screen.getByRole('button', { name: /close terminal tab biorouter 2/i }));

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(1);
    });
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes the dock when users close the last terminal tab', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={onClose} />);

    await waitFor(() => {
      expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter');
    });

    await user.click(screen.getByRole('button', { name: /close terminal tab biorouter/i }));

    await waitFor(() => {
      expect(screen.queryByRole('tab')).not.toBeInTheDocument();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not hijack keyboard activation from dock or surrounding controls', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onOutsideAction = vi.fn();

    render(
      <>
        <button type="button" onClick={onOutsideAction}>
          Outside action
        </button>
        <InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={onClose} />
      </>
    );

    await waitFor(() => {
      expect(window.electron.createTerminalSession).toHaveBeenCalled();
    });

    screen.getByRole('button', { name: 'Outside action' }).focus();
    await user.keyboard('{Enter}');
    expect(onOutsideAction).toHaveBeenCalledTimes(1);
    expect(window.electron.writeTerminalSession).not.toHaveBeenCalled();

    screen.getByRole('button', { name: 'Hide terminal' }).focus();
    await user.keyboard('{Enter}');
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('caps concurrent terminal panes and keeps the tab strip scrollable', async () => {
    const user = userEvent.setup();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />);

    const addButton = await screen.findByRole('button', { name: /new terminal/i });
    for (let index = 1; index < MAX_TERMINAL_PANES; index += 1) {
      await user.click(addButton);
    }

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(MAX_TERMINAL_PANES);
    });
    expect(addButton).toBeDisabled();
    expect(screen.getByRole('tablist', { name: /^terminals$/i })).toHaveClass('overflow-x-auto');
  });

  // "Run" on a shell code block in the transcript (utils/terminalRunChannel.ts).
  // The click happens far away in the chat, so the registry IS the entry point
  // here exactly as it is for Cmd+W above: runInTerminal() is what BaseChat
  // calls, and the only thing between it and a real pty is this pane.
  describe('running a code block from the chat', () => {
    const ESC = String.fromCharCode(0x1b);

    async function renderDock(props: Partial<{ dockKey: string; open: boolean }> = {}) {
      const view = render(
        <InAppTerminalDock
          dockKey="tab-1"
          open
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={vi.fn()}
          {...props}
        />
      );
      await waitFor(() => expect(window.electron.createTerminalSession).toHaveBeenCalled());
      // The pty id is assigned after the async spawn resolves; until it is,
      // writes go to pendingInputRef instead of the backend.
      await waitFor(() => expect(xtermInstances.length).toBeGreaterThan(0));
      return view;
    }

    it("writes the command into this dock's shell and SUBMITS it", async () => {
      await renderDock();

      act(() => runInTerminal('tab-1', 'ls -la'));

      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith('pty-1', 'ls -la\r')
      );
    });

    it('brackets the paste when the SHELL has asked for it', async () => {
      await renderDock();
      xtermInstances[0].modes.bracketedPasteMode = true;

      act(() => runInTerminal('tab-1', 'ls -la'));

      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith(
          'pty-1',
          `${ESC}[200~ls -la${ESC}[201~\r`
        )
      );
    });

    it('sends a multi-line block as ONE buffer with a single Enter', async () => {
      // Line-by-line, each newline of a heredoc is its own Enter. Bracketed,
      // the whole thing lands in the line editor and the trailing CR runs it.
      await renderDock();
      xtermInstances[0].modes.bracketedPasteMode = true;

      act(() => runInTerminal('tab-1', 'cat <<EOF\nhello\nEOF'));

      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith(
          'pty-1',
          `${ESC}[200~cat <<EOF\rhello\rEOF${ESC}[201~\r`
        )
      );
    });

    it('focuses the terminal, so the user can Ctrl-C what they started', async () => {
      await renderDock();

      act(() => runInTerminal('tab-1', 'sleep 60'));

      await waitFor(() => expect(xtermInstances[0].focus).toHaveBeenCalled());
    });

    it('delivers a command clicked BEFORE any terminal existed', async () => {
      // The ordinary case: no dock is open, so the click both opens one and
      // asks it to run something — three commits before a pane exists.
      const { rerender } = render(
        <InAppTerminalDock
          dockKey="tab-1"
          open={false}
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={vi.fn()}
        />
      );
      expect(document.querySelector('[data-terminal-pane]')).toBeNull();

      act(() => runInTerminal('tab-1', 'ls -la'));
      expect(window.electron.writeTerminalSession).not.toHaveBeenCalled();

      rerender(
        <InAppTerminalDock
          dockKey="tab-1"
          open
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={vi.fn()}
        />
      );

      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith('pty-1', 'ls -la\r')
      );
    });

    it('lands in the ACTIVE pane, not the first one', async () => {
      const user = userEvent.setup();
      await renderDock();

      await user.click(screen.getByRole('button', { name: /new terminal/i }));
      await waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(2));
      await waitFor(() => expect(window.electron.createTerminalSession).toHaveBeenCalledTimes(2));

      act(() => runInTerminal('tab-1', 'ls -la'));

      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith('pty-2', 'ls -la\r')
      );
      expect(window.electron.writeTerminalSession).not.toHaveBeenCalledWith('pty-1', 'ls -la\r');
    });

    it('ignores a request for ANOTHER chat tab', async () => {
      await renderDock();

      act(() => runInTerminal('tab-2', 'rm -rf build'));

      await waitFor(() => expect(xtermInstances.length).toBe(1));
      expect(window.electron.writeTerminalSession).not.toHaveBeenCalled();
    });

    it("a dock with no key — the onboarding card's — never receives one", async () => {
      // That dock belongs to no chat, and a transcript\'s Run button must never
      // find it.
      await renderDock({ dockKey: undefined });

      act(() => runInTerminal('tab-1', 'ls -la'));

      expect(window.electron.writeTerminalSession).not.toHaveBeenCalled();
    });
  });

  // Issue #21 — the Cmd+W ladder's first rung. The keystroke never reaches the
  // DOM (the Electron menu owns Cmd+W and delivers IPC), so the registry IS the
  // keyboard's entry point: requestCloseTerminalPane() here is exactly what
  // runCloseActiveTabCommand calls when the terminal has focus.
  describe('Cmd+W via the close-pane registry', () => {
    it('closes the ACTIVE pane and keeps the dock when others remain', async () => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const onEmptied = vi.fn();

      render(
        <InAppTerminalDock
          open
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={onClose}
          onEmptied={onEmptied}
        />
      );

      await user.click(await screen.findByRole('button', { name: /new terminal/i }));
      await waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(2));
      // The newest pane is the active one — precisely the pane Cmd+W must take.
      expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter 2');

      let claimed = false;
      act(() => {
        claimed = requestCloseTerminalPane();
      });

      expect(claimed).toBe(true);
      await waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(1));
      expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter');
      expect(onClose).not.toHaveBeenCalled();
      expect(onEmptied).not.toHaveBeenCalled();
    });

    it('closing the last pane destroys the dock (onEmptied), still claiming the key', async () => {
      const onClose = vi.fn();
      const onEmptied = vi.fn();

      render(
        <InAppTerminalDock
          open
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={onClose}
          onEmptied={onEmptied}
        />
      );
      await screen.findByRole('tab', { selected: true });

      let claimed = false;
      act(() => {
        claimed = requestCloseTerminalPane();
      });

      expect(claimed).toBe(true);
      await waitFor(() => expect(screen.queryByRole('tab')).not.toBeInTheDocument());
      expect(onEmptied).toHaveBeenCalledTimes(1);
      expect(onClose).not.toHaveBeenCalled();
    });

    it('a HIDDEN dock does not register — the request falls through to the chat tab', async () => {
      const { rerender } = render(
        <InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />
      );
      await screen.findByRole('tab', { selected: true });

      rerender(
        <InAppTerminalDock
          open={false}
          workingDir="/Users/wgu/Desktop/biorouter"
          onClose={vi.fn()}
        />
      );

      let claimed = true;
      act(() => {
        claimed = requestCloseTerminalPane();
      });
      expect(claimed).toBe(false);
      // The hidden dock's panes survive — hiding is not closing.
      expect(screen.getAllByRole('tab', { hidden: true })).toHaveLength(1);
    });
  });

  // Folding the dock away and bringing it back must not cost the user their
  // scrollback — the thing that makes the in-app terminal usable at all, and the
  // first thing anyone checks after clicking Run and then hiding the dock.
  //
  // Scrollback (8000 lines) lives in the live XTerm instance, and the pty lives
  // in the main process keyed by session id. So the property to pin is that
  // NEITHER is replaced: hide is a `hidden` class on a still-mounted tree, and
  // the teardown effect keys on paneId/workingDir, not on `open`.
  describe('folding the dock away', () => {
    it('keeps the same terminal and the same shell across hide and show', async () => {
      const props = { workingDir: '/Users/wgu/Desktop/biorouter', onClose: vi.fn() };
      const { rerender } = render(<InAppTerminalDock open {...props} />);
      await waitFor(() => expect(xtermInstances.length).toBe(1));
      const [terminal] = xtermInstances;
      const pane = document.querySelector('[data-terminal-pane]');

      rerender(<InAppTerminalDock open={false} {...props} />);

      // Still mounted, merely hidden — the section carries `hidden`, the pane
      // itself is untouched. A dock that unmounted here would take the pty with
      // it through the teardown effect.
      expect(screen.getByTestId('in-app-terminal-dock')).toHaveClass('hidden');
      expect(document.querySelector('[data-terminal-pane]')).toBe(pane);
      expect(window.electron.disposeTerminalSession).not.toHaveBeenCalled();

      rerender(<InAppTerminalDock open {...props} />);

      expect(screen.getByTestId('in-app-terminal-dock')).not.toHaveClass('hidden');
      // The SAME instance, so its 8000-line buffer is the same buffer. A
      // recreated pane would look identical in the DOM and be empty.
      expect(xtermInstances).toHaveLength(1);
      expect(xtermInstances[0]).toBe(terminal);
      expect(window.electron.createTerminalSession).toHaveBeenCalledTimes(1);
      expect(window.electron.disposeTerminalSession).not.toHaveBeenCalled();
    });

    it('a command run before folding is still running in the same shell after', async () => {
      const props = {
        dockKey: 'tab-1',
        workingDir: '/Users/wgu/Desktop/biorouter',
        onClose: vi.fn(),
      };
      const { rerender } = render(<InAppTerminalDock open {...props} />);
      await waitFor(() => expect(window.electron.createTerminalSession).toHaveBeenCalled());
      await waitFor(() => expect(xtermInstances.length).toBe(1));

      act(() => runInTerminal('tab-1', 'ls -la'));
      await waitFor(() =>
        expect(window.electron.writeTerminalSession).toHaveBeenCalledWith('pty-1', 'ls -la\r')
      );

      rerender(<InAppTerminalDock open={false} {...props} />);
      rerender(<InAppTerminalDock open {...props} />);

      // Same pty, never disposed and never respawned: the command's output is
      // still in the buffer the user comes back to.
      expect(window.electron.disposeTerminalSession).not.toHaveBeenCalled();
      expect(window.electron.createTerminalSession).toHaveBeenCalledTimes(1);
    });
  });

  // The terminal used to be a rounded, bordered box painted bg-background-muted
  // sitting inside a gutter also painted bg-background-muted — a hairline drawn
  // between a surface and itself. The dock's own border-t is the only hairline
  // the terminal needs (D-11), so the host must stay a plain, unpainted inset.
  it('renders the terminal host without a nested bordered box', async () => {
    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />);

    const pane = await waitFor(() => {
      const el = document.querySelector('[data-terminal-pane]');
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });

    const host = pane.firstElementChild as HTMLElement;
    // The host is still an inset — xterm needs room to breathe, just not a box.
    expect(host).toHaveClass('px-2', 'py-1.5', 'overflow-hidden');
    for (const boxClass of ['rounded-md', 'border', 'border-border-subtle']) {
      expect(host).not.toHaveClass(boxClass);
    }
    // The ground is painted once, by the region above the host — never twice.
    expect(host).not.toHaveClass('bg-background-muted');

    const region = pane.parentElement as HTMLElement;
    // The ground is the FAMILY'S declared `terminalGround` token, not a
    // hardcoded utility. All six family/mode combinations resolve to
    // `--background-muted` today, so the old `toHaveClass('bg-background-muted')`
    // passed for the wrong reason: it would have kept passing while the first
    // family to move its ground left a rim of the old surface around a
    // re-grounded xterm canvas. Assert the wiring, not the current value.
    expect(region.dataset.terminalGround).toBe(GENERATED_THEMES.parchment.light.terminalGround);
    expect(region.style.background).toBe(`var(${GENERATED_THEMES.parchment.light.terminalGround})`);
    expect(region).not.toHaveClass('bg-background-muted');
    expect(region).not.toHaveClass('p-2'); // no gutter: the terminal bleeds to the dock edges
  });

  // The "+" is LEFT-aligned: it lives inside the tab strip's scroll box, right
  // after the last tab, and scrolls with the tabs — the same placement as the
  // chat tabs' new-tab button. It used to be pushed to the far right of the row.
  it('places the new-terminal "+" inside the tab strip, after the last tab', async () => {
    const user = userEvent.setup();
    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />);

    const tabList = screen.getByRole('tablist', { name: /^terminals$/i });
    const addButton = await screen.findByRole('button', { name: /new terminal/i });

    // Inside the scrolling tab strip (not a right-hand toolbar).
    expect(tabList.contains(addButton)).toBe(true);
    expect(addButton).toHaveClass('br-tab-new');

    await user.click(addButton);
    await waitFor(() => expect(screen.getAllByRole('tab')).toHaveLength(2));

    // In DOM order the "+" comes after every tab.
    const allTabs = screen.getAllByRole('tab');
    const lastTab = allTabs[allTabs.length - 1].closest('.br-tab') as HTMLElement;
    expect(lastTab.compareDocumentPosition(addButton) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );
  });

  // design.md D-07: the left accent bar is for VERTICAL lists only. These are
  // horizontal tabs, so they use the shared Safari-style tab classes; the pill,
  // the divider and the strip ground all come from main.css, never from here.
  it('styles terminal tabs with the shared Safari tab classes', async () => {
    const user = userEvent.setup();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={vi.fn()} />);

    const tabList = screen.getByRole('tablist', { name: /^terminals$/i });
    const strip = tabList.closest('.br-tabstrip') as HTMLElement;
    expect(strip).not.toBeNull();
    expect(strip).toHaveClass('br-tabstrip--sm');
    // The strip's ground, padding, gap and bottom hairline all come from the
    // class. Overriding any of them locally is what this guards against.
    expect(strip.className).not.toMatch(/bg-background|border-b|px-|gap-/);

    await user.click(screen.getByRole('button', { name: /new terminal/i }));
    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });

    const tabs = screen.getAllByRole('tab').map((tab) => tab.closest('.br-tab') as HTMLElement);
    expect(tabs.every(Boolean)).toBe(true);
    // Only the active tab is painted, and only via data-active.
    expect(tabs.map((tab) => tab.dataset.active)).toEqual(['false', 'true']);

    for (const tab of tabs) {
      expect(tab.querySelector('.br-tab__label')).not.toBeNull();
      // No local restyling: no left accent bar (D-07 reserves it for vertical
      // lists), no chip, no separators, no width/padding overrides.
      expect(tab.className).not.toMatch(
        /before:|rounded|accent-bar|bg-background|max-w-|min-w-|px-|h-\d/
      );
    }
  });
});
