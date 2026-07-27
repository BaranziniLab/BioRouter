import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// #44 — the empty-chat-only working-directory rule at the chip level. While
// the chat is completely empty the chip is the chooser (both the pre-session
// #39 path and a fresh zero-message session); once the chat has messages it
// is a read-only label — basename only, full path on hover — that can never
// reach updateWorkingDir.

vi.mock('../../api', () => ({
  updateWorkingDir: vi.fn(async () => ({ data: {} })),
}));
vi.mock('../../toasts', () => ({
  toastError: vi.fn(),
}));

import { DirSwitcher, workingDirLabel } from './DirSwitcher';
import { updateWorkingDir } from '../../api';
import { toastError } from '../../toasts';

const WORKING_DIR = '/Users/wgu/Desktop';
const CHOSEN_DIR = '/Users/wgu/Desktop/data';

beforeAll(() => {
  // Radix tooltip measures its trigger.
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, {
    electron: {
      directoryChooser: vi.fn(async () => ({ canceled: false, filePaths: [CHOSEN_DIR] })),
      addRecentDir: vi.fn(),
      openDirectoryInExplorer: vi.fn(),
    },
  });
});

describe('workingDirLabel', () => {
  it('shows only the basename of a directory', () => {
    expect(workingDirLabel('/Users/wgu/Desktop')).toBe('Desktop');
    expect(workingDirLabel('/Users/wgu/Desktop/')).toBe('Desktop');
    expect(workingDirLabel('C:\\Users\\wgu\\Desktop')).toBe('Desktop');
  });

  it('shows the home directory as its own basename', () => {
    expect(workingDirLabel('/Users/wgu')).toBe('wgu');
  });

  it('shows a rootlike path as-is when there is no basename', () => {
    expect(workingDirLabel('/')).toBe('/');
    expect(workingDirLabel('')).toBe('');
  });

  it('shows a Windows drive root as-is, not as a folder named "C:"', () => {
    expect(workingDirLabel('C:\\')).toBe('C:\\');
    expect(workingDirLabel('C:/')).toBe('C:/');
    expect(workingDirLabel('d:\\\\')).toBe('d:\\\\');
  });
});

describe('DirSwitcher while the chat is empty', () => {
  it('opens the chooser and persists the choice for a zero-message session', async () => {
    const onWorkingDirChange = vi.fn();
    render(
      <DirSwitcher
        className=""
        sessionId="session-1"
        workingDir={WORKING_DIR}
        locked={false}
        onWorkingDirChange={onWorkingDirChange}
      />
    );

    // Interactive chip: a real button showing the full path.
    const chip = screen.getByRole('button');
    expect(chip).toHaveTextContent(WORKING_DIR);
    fireEvent.click(chip);

    // throwOnError is required: without it the generated client resolves with
    // an error object on a 409 and the failure path would never run.
    await waitFor(() =>
      expect(updateWorkingDir).toHaveBeenCalledWith({
        body: { session_id: 'session-1', working_dir: CHOSEN_DIR },
        throwOnError: true,
      })
    );
    // The displayed dir and the recents list update only after success.
    expect(onWorkingDirChange).toHaveBeenCalledWith(CHOSEN_DIR);
    expect(window.electron.addRecentDir).toHaveBeenCalledWith(CHOSEN_DIR);
  });

  it('leaves the displayed dir and recents untouched when the server refuses (409)', async () => {
    vi.mocked(updateWorkingDir).mockRejectedValueOnce({
      message: 'the working directory is fixed once a chat has messages',
    });
    const onWorkingDirChange = vi.fn();
    const onRestartEnd = vi.fn();
    render(
      <DirSwitcher
        className=""
        sessionId="session-1"
        workingDir={WORKING_DIR}
        locked={false}
        onWorkingDirChange={onWorkingDirChange}
        onRestartEnd={onRestartEnd}
      />
    );

    fireEvent.click(screen.getByRole('button'));

    await waitFor(() => expect(updateWorkingDir).toHaveBeenCalled());
    await waitFor(() => expect(onRestartEnd).toHaveBeenCalled());

    // Refused server-side: nothing may change client-side.
    expect(onWorkingDirChange).not.toHaveBeenCalled();
    expect(window.electron.addRecentDir).not.toHaveBeenCalled();
    expect(toastError).toHaveBeenCalledWith({
      title: 'Working directory update failed',
      msg: 'the working directory is fixed once a chat has messages',
    });
  });

  it('keeps the #39 pre-session path: forwards the choice without persisting', async () => {
    const onWorkingDirChange = vi.fn();
    render(
      <DirSwitcher
        className=""
        sessionId={undefined}
        workingDir={WORKING_DIR}
        onWorkingDirChange={onWorkingDirChange}
      />
    );

    fireEvent.click(screen.getByRole('button'));

    await waitFor(() => expect(onWorkingDirChange).toHaveBeenCalledWith(CHOSEN_DIR));
    expect(updateWorkingDir).not.toHaveBeenCalled();
  });
});

describe('DirSwitcher once the chat has messages (locked, #44)', () => {
  it('renders a read-only basename label with the full path on hover', async () => {
    const user = userEvent.setup();
    render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={true} />
    );

    // Basename only, and no interactive affordance at all.
    const label = screen.getByTestId('dir-switcher-locked');
    expect(label).toHaveTextContent('Desktop');
    expect(label).not.toHaveTextContent(WORKING_DIR);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    // The full path is still discoverable on hover.
    await user.hover(label);
    expect(await screen.findByRole('tooltip')).toHaveTextContent(WORKING_DIR);
  });

  it('never opens the chooser or calls updateWorkingDir', async () => {
    const onWorkingDirChange = vi.fn();
    render(
      <DirSwitcher
        className=""
        sessionId="session-1"
        workingDir={WORKING_DIR}
        locked={true}
        onWorkingDirChange={onWorkingDirChange}
      />
    );

    fireEvent.click(screen.getByTestId('dir-switcher-locked'));

    // Nothing may happen — no chooser, no persistence, no local change.
    await waitFor(() => expect(window.electron.directoryChooser).not.toHaveBeenCalled());
    expect(updateWorkingDir).not.toHaveBeenCalled();
    expect(onWorkingDirChange).not.toHaveBeenCalled();
  });
});
