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
// Issue #56 Task 58 / #47: `/agent/update_working_dir` names a chat, and
// repointing a private one needs the proof-of-user. The assertion below names
// the header rather than tolerating whatever is there.
vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

import { DirSwitcher, workingDirLabel, deriveWorkingDirLocked } from './DirSwitcher';
import { ChatState } from '../../types/chatState';
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

/**
 * The unlocked chip shows the FULL path, left-truncated, so the directory is
 * readable while it is still being chosen. Two facts about it are assertable
 * here; the third is not.
 *
 * ⚠ **jsdom cannot see either the width cap or the bidi ordering** — it has no
 * layout engine and does not implement the Unicode bidirectional algorithm, so
 * `textContent` reads back the logical string whether the box is 112px or 46ch
 * and whether the leading separator is drawn at the far end or not. What is
 * assertable here is the DOM contract: the exact string reaches the chip
 * unmangled, and it reaches it inside the isolation element the stylesheet
 * selects. The width and the visual glyph order are pinned at the source in
 * `styles/dirChipPath.test.ts` and were measured in a real browser.
 */
describe('the chip renders the whole path, inside its bidi isolate', () => {
  const chipPath = (container: HTMLElement) =>
    container.querySelector<HTMLElement>('.biorouter-dir-chip-path');

  it.each([
    ['an ordinary path', '/Users/wgu/Downloads'],
    // A path whose trailing slash is REAL must keep it, and one without a
    // trailing slash must not grow one — the shipped RTL box reordered the
    // LEADING separator to the visual right end and made these two
    // indistinguishable, showing `…wgu/Downloads/` for the path below.
    ['a path with a real trailing slash', '/Users/wgu/Downloads/'],
    ['a leading-slash root', '/'],
    // Backslashes are bidi-neutral too: `C:\` rendered `\:C`, fully reversed.
    ['a Windows path', 'C:\\Users\\wgu\\Desktop'],
    ['a Windows drive root', 'C:\\'],
  ])('shows %s exactly as it is, inside a <bdi>', (_label, dir) => {
    const { container } = render(
      <DirSwitcher className="" sessionId={undefined} workingDir={dir} locked={false} />
    );

    const path = chipPath(container);
    expect(path).not.toBeNull();
    expect(path!.textContent).toBe(dir);

    // The stylesheet's LTR isolate is `.biorouter-dir-chip-path > bdi`; without
    // the element the rule matches nothing and the separators reorder again.
    const bdi = path!.querySelector('bdi');
    expect(bdi).not.toBeNull();
    expect(bdi!.textContent).toBe(dir);
  });

  it('leaves the locked chip on its basename, with no isolate and no head clipping', () => {
    const { container } = render(
      <DirSwitcher className="" sessionId="session-1" workingDir="/Users/wgu/Downloads" locked />
    );

    expect(screen.getByTestId('dir-switcher-locked')).toHaveTextContent('Downloads');
    expect(chipPath(container)).toBeNull();
    expect(container.querySelector('bdi')).toBeNull();
  });
});

// #44 — the authoritative lock derivation the chip's `locked` prop is fed
// from. The two store-shaped scenarios that motivated it: a resumed transcript
// that has not hydrated yet (messages.length 0 for a non-empty session) must
// be locked from first paint, and a failed optimistic first submit (transcript
// retains the unsent message) must stay unlocked.
describe('deriveWorkingDirLocked', () => {
  const base = {
    sessionId: 'session-1' as string | null | undefined,
    persistedMessageCount: 0 as number | undefined,
    hasAssistantMessage: false,
    chatState: ChatState.Idle,
  };

  it('never locks before a session exists (#39 pre-session chooser)', () => {
    expect(deriveWorkingDirLocked({ ...base, sessionId: null })).toBe(false);
    expect(deriveWorkingDirLocked({ ...base, sessionId: undefined })).toBe(false);
  });

  it('locks a resumed session from first paint until history proves empty', () => {
    // Metadata still loading: assume locked.
    expect(deriveWorkingDirLocked({ ...base, persistedMessageCount: undefined })).toBe(true);
    // Metadata arrived and reports messages, even though the transcript is
    // still hydrating (the store loads it 0 -> N atomically).
    expect(
      deriveWorkingDirLocked({
        ...base,
        persistedMessageCount: 3,
        chatState: ChatState.LoadingConversation,
      })
    ).toBe(true);
    // History proved empty: unlock.
    expect(deriveWorkingDirLocked({ ...base, persistedMessageCount: 0 })).toBe(false);
  });

  it('stays unlocked after a failed first submit whose optimistic message never reached the server', () => {
    // The store keeps the unsent message in the transcript by design (so it
    // goes out when the agent lands); the turn errored back to Idle and the
    // server still reports zero messages -> the dir is still choosable.
    expect(
      deriveWorkingDirLocked({
        ...base,
        persistedMessageCount: 0,
        hasAssistantMessage: false,
        chatState: ChatState.Idle,
      })
    ).toBe(false);
  });

  it('locks while the first turn is in flight and once any assistant reply exists', () => {
    // A turn actively streaming means the first message is reaching the
    // server now, even though the last session fetch said zero.
    expect(deriveWorkingDirLocked({ ...base, chatState: ChatState.Streaming })).toBe(true);
    expect(deriveWorkingDirLocked({ ...base, chatState: ChatState.Thinking })).toBe(true);
    // An assistant reply proves a message reached the server (stale count).
    expect(deriveWorkingDirLocked({ ...base, hasAssistantMessage: true })).toBe(true);
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
        headers: { 'X-User-Action': 'test-key' },
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

// #50 — the chip's Tooltip must keep ONE control mode for its whole life.
// Both branches render `<TooltipProvider><Tooltip>` in the same position, so
// React reconciles them into a single Tooltip instance; when the working
// directory locks after the first message the instance would flip from
// controlled (`open`/`onOpenChange`) to uncontrolled. Radix warns about it,
// and the state it warns about is the one that later yields a stuck-open or
// unresponsive tooltip.
describe('DirSwitcher tooltip control mode (#50)', () => {
  const CONTROL_MODE_WARNING = /changing from (controlled|uncontrolled) to/;

  it('does not flip the tooltip between controlled and uncontrolled when the dir locks', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const { rerender } = render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={false} />
    );
    // The chat gets its first message: the same chip becomes read-only.
    rerender(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={true} />
    );
    // ...and a later unlock (new empty session in the same chip) must not flip back.
    rerender(
      <DirSwitcher className="" sessionId="session-2" workingDir={WORKING_DIR} locked={false} />
    );

    await waitFor(() => expect(screen.getByRole('button')).toBeInTheDocument());

    const controlModeWarnings = warn.mock.calls.filter((call) =>
      call.some((arg) => typeof arg === 'string' && CONTROL_MODE_WARNING.test(arg))
    );
    expect(controlModeWarnings).toEqual([]);

    warn.mockRestore();
  });

  it('still reveals the full path on hover once locked', async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={false} />
    );
    rerender(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={true} />
    );

    await user.hover(screen.getByTestId('dir-switcher-locked'));
    expect(await screen.findByRole('tooltip')).toHaveTextContent(WORKING_DIR);
  });
});

/// A working directory is a PATH, and paths are monospace across the app:
/// RecentChats.tsx (the sidebar tooltip), SessionHistoryView.tsx and
/// SharedSessionView.tsx all set this same value in `font-mono`. The composer
/// chip was the one body-font holdout — and it sits on screen at the same time
/// as the sidebar, so hovering a recent chat showed a mono path inches from a
/// body-font one.
///
/// jsdom never runs Tailwind, so a computed-font assertion would pass whatever
/// the class says. These assert the CLASS on the element, and walk ancestors
/// where the face could be inherited instead.
describe('DirSwitcher — the working directory is a path, so it is monospace', () => {
  it('sets the unlocked chip in monospace', () => {
    render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={false} />
    );
    // The path sits one level deeper than it used to — inside the `<bdi>` that
    // isolates it as LTR — so the face is inherited from the chip rather than
    // carried on the text's own element. `closest` is the "walk ancestors"
    // case this block's comment already allows for; asserting on the innermost
    // element would fail while the chip is correctly monospace.
    expect(screen.getByText(WORKING_DIR).closest('.font-mono')).not.toBeNull();
  });

  it('sets the locked basename label in monospace', () => {
    render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={true} />
    );
    // Same component, so the face must not flip when the chat locks.
    expect(screen.getByText('Desktop').className).toMatch(/font-mono/);
  });

  it('sets the hover tooltip in monospace, overriding the tooltip font-sans', async () => {
    const user = userEvent.setup();
    render(
      <DirSwitcher className="" sessionId="session-1" workingDir={WORKING_DIR} locked={true} />
    );

    await user.hover(screen.getByTestId('dir-switcher-locked'));
    const tooltip = await screen.findByRole('tooltip');

    // `ui/Tooltip.tsx` puts `font-sans` on the content box itself, so the
    // override must be on a descendant — asserting on the box would pass while
    // the path still rendered in the body font.
    const path = [...tooltip.querySelectorAll('*')].find(
      (node) => node.textContent === WORKING_DIR
    );
    expect(path).toBeDefined();
    expect(path!.className).toMatch(/font-mono/);
  });
});
