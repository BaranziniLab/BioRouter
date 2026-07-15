import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import InAppTerminalDock, { MAX_TERMINAL_PANES } from './InAppTerminalDock';

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    dispose = vi.fn();
    focus = vi.fn();
    loadAddon = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    open = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
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
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      createTerminalSession: vi.fn(async () => ({
        backend: 'pty',
        cwd: '/Users/wgu/Desktop/biorouter',
        sessionId: window.crypto.randomUUID(),
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

    const tabList = screen.getByRole('tablist', { name: /terminal sessions/i });
    expect(tabList).toBeInTheDocument();
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('hidden');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('invisible');

    await user.click(screen.getByRole('button', { name: /new terminal session/i }));

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('biorouter 2');
  });

  it('closes only the selected terminal tab and keeps another tab active', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/biorouter" onClose={onClose} />);

    await user.click(screen.getByRole('button', { name: /new terminal session/i }));
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

    const addButton = await screen.findByRole('button', { name: /new terminal session/i });
    for (let index = 1; index < MAX_TERMINAL_PANES; index += 1) {
      await user.click(addButton);
    }

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(MAX_TERMINAL_PANES);
    });
    expect(addButton).toBeDisabled();
    expect(screen.getByRole('tablist', { name: /terminal sessions/i })).toHaveClass(
      'overflow-x-auto'
    );
  });
});
