import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import InAppTerminalDock from './InAppTerminalDock';

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
        cwd: '/Users/wgu/Desktop/BioRouter',
        sessionId: crypto.randomUUID(),
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

    render(
      <InAppTerminalDock open workingDir="/Users/wgu/Desktop/BioRouter" onClose={vi.fn()} />
    );

    const tabList = screen.getByRole('tablist', { name: /terminal sessions/i });
    expect(tabList).toBeInTheDocument();
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('BioRouter');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('hidden');
    expect(document.querySelector('[data-terminal-pane]')).not.toHaveClass('invisible');

    await user.click(screen.getByRole('button', { name: /new terminal session/i }));

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('BioRouter 2');
  });

  it('closes only the selected terminal tab and keeps another tab active', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/BioRouter" onClose={onClose} />);

    await user.click(screen.getByRole('button', { name: /new terminal session/i }));
    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });

    await user.click(screen.getByRole('button', { name: /close terminal tab biorouter 2/i }));

    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(1);
    });
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('BioRouter');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes the dock when users close the last terminal tab', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<InAppTerminalDock open workingDir="/Users/wgu/Desktop/BioRouter" onClose={onClose} />);

    await waitFor(() => {
      expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('BioRouter');
    });

    await user.click(screen.getByRole('button', { name: /close terminal tab biorouter/i }));

    await waitFor(() => {
      expect(screen.queryByRole('tab')).not.toBeInTheDocument();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('forwards plain keystrokes to the active terminal when focus stays on the window chrome', async () => {
    render(
      <InAppTerminalDock open workingDir="/Users/wgu/Desktop/BioRouter" onClose={vi.fn()} />
    );

    await waitFor(() => {
      expect(window.electron.createTerminalSession).toHaveBeenCalled();
    });

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', bubbles: true }));

    await waitFor(() => {
      expect(window.electron.writeTerminalSession).toHaveBeenCalledWith(expect.any(String), 'p');
    });
  });
});
