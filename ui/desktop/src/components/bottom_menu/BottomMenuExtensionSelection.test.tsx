import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomMenuExtensionSelection } from './BottomMenuExtensionSelection';

const mocks = vi.hoisted(() => ({
  overrides: new Map<string, boolean>(),
  getSessionExtensions: vi.fn(async () => ({ data: { extensions: [] } })),
  addToAgent: vi.fn(async (): Promise<void> => undefined),
  removeFromAgent: vi.fn(async (): Promise<void> => undefined),
}));

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: [
      {
        type: 'builtin',
        name: 'autovisualiser',
        display_name: 'Auto Visualiser',
        description: 'Visualization capability',
        enabled: true,
      },
      {
        type: 'platform',
        name: 'code_execution',
        description: 'Code execution capability',
        enabled: true,
      },
      {
        type: 'platform',
        name: 'chatrecall',
        description: 'Chat recall capability',
        enabled: false,
      },
      {
        type: 'builtin',
        name: 'agent_drafter',
        description: 'Agent drafter capability',
        enabled: false,
      },
      {
        type: 'stdio',
        name: 'example',
        display_name: 'Example',
        description: 'Example extension',
        cmd: 'example',
        args: [],
        enabled: false,
      },
    ],
  }),
}));

vi.mock('../settings/extensions/subcomponents/ExtensionList', () => ({
  formatExtensionName: (name: string) => name,
  isBuiltInExtension: () => false,
}));

vi.mock('../../api', () => ({
  getSessionExtensions: mocks.getSessionExtensions,
}));

vi.mock('../settings/extensions/agent-api', () => ({
  addToAgent: mocks.addToAgent,
  removeFromAgent: mocks.removeFromAgent,
}));

vi.mock('../../store/extensionOverrides', () => ({
  setExtensionOverride: (name: string, enabled: boolean) => mocks.overrides.set(name, enabled),
  getExtensionOverrides: () => mocks.overrides,
}));

vi.mock('../../toasts', () => ({
  toastService: { success: vi.fn(), error: vi.fn() },
}));

describe('BottomMenuExtensionSelection', () => {
  beforeEach(() => {
    mocks.overrides.clear();
    vi.clearAllMocks();
    // `clearAllMocks` clears CALLS, not implementations, so a `mockResolvedValue`
    // set by one test would otherwise be the starting state of the next.
    mocks.getSessionExtensions.mockReset();
    mocks.getSessionExtensions.mockResolvedValue({ data: { extensions: [] } } as never);
  });

  it('keeps an immediate hub toggle when the menu closes and reopens', async () => {
    render(<BottomMenuExtensionSelection sessionId={null} />);
    const trigger = screen.getByLabelText(/Manage extensions/);
    expect(trigger).not.toHaveAttribute('title');
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });

    const toggle = await screen.findByRole('menuitemcheckbox');
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));

    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('menuitemcheckbox')).not.toBeInTheDocument());
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });

    const reopenedToggle = await screen.findByRole('menuitemcheckbox');
    expect(reopenedToggle).toHaveAttribute('aria-checked', 'true');
    fireEvent.click(reopenedToggle);
    await waitFor(() => expect(reopenedToggle).toHaveAttribute('aria-checked', 'false'));
    expect(mocks.overrides.get('example')).toBe(false);
  });

  /**
   * v1.89.0 P-04. The chip counts the chat's extensions; the menu lists the ones
   * this menu may toggle. They are different sets by design — capabilities are
   * managed in Settings → Chat — and counting the menu made the chip read
   * `(0 enabled)` on a chat with eleven extensions loaded and working.
   *
   * ⚠ The fixture is chosen so a regression cannot pass: every capability here
   * is enabled and the only non-capability extension is DISABLED, so counting
   * the menu gives 0 and counting the chat gives 2. A fixture where the two
   * happen to agree proves nothing.
   */
  it('counts the chat`s extensions, not the rows this menu happens to show', async () => {
    mocks.getSessionExtensions.mockResolvedValue({
      data: {
        extensions: [
          { type: 'builtin', name: 'autovisualiser' },
          { type: 'platform', name: 'code_execution' },
        ],
      },
    } as never);

    render(<BottomMenuExtensionSelection sessionId="session-1" />);

    await waitFor(() =>
      expect(screen.getByLabelText('Manage extensions (2 enabled)')).toBeInTheDocument()
    );

    // …and the menu still shows only the one togglable row.
    fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
      button: 0,
      ctrlKey: false,
    });
    expect(await screen.findAllByRole('menuitemcheckbox')).toHaveLength(1);
  });

  it('counts the config`s enabled extensions before the session read lands', async () => {
    // The daemon applies exactly this fallback for a session with no stored
    // extension state, so the chip must not flash 0 on the way to the number.
    mocks.getSessionExtensions.mockReturnValue(new Promise(() => {}) as never);

    render(<BottomMenuExtensionSelection sessionId="session-1" />);

    // autovisualiser + code_execution are enabled in the fixture; chatrecall,
    // agent_drafter and example are not.
    expect(screen.getByLabelText('Manage extensions (2 enabled)')).toBeInTheDocument();
  });

  it('moves the chip the moment a row is toggled, before the refetch', async () => {
    mocks.getSessionExtensions.mockResolvedValue({
      data: { extensions: [{ type: 'builtin', name: 'autovisualiser' }] },
    } as never);
    let resolveEnable: (() => void) | undefined;
    mocks.addToAgent.mockImplementationOnce(
      () => new Promise<void>((resolve) => (resolveEnable = resolve))
    );

    render(<BottomMenuExtensionSelection sessionId="session-1" />);
    await waitFor(() =>
      expect(screen.getByLabelText('Manage extensions (1 enabled)')).toBeInTheDocument()
    );

    fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole('menuitemcheckbox'));

    await waitFor(() =>
      expect(screen.getByLabelText('Manage extensions (2 enabled)')).toBeInTheDocument()
    );
    resolveEnable?.();
  });

  /**
   * v1.89.0 P-03. Toggling Workspace on here, quitting, and finding it off again
   * reads as a setting that failed to save. It is a per-chat control being read
   * as a global one: `/agent/add_extension` persists into that SESSION's own
   * extension state and never touches `config.yaml`, and the hub view's
   * overrides live in a map `createSession` clears. Making it write the config
   * instead would be the wrong repair — disabling an extension in one chat would
   * disable it everywhere — so the menu has to say what it does.
   *
   * ⚠ Asserted on the SCOPE WORD, not on the sentence, because the two views say
   * genuinely different things and a single phrase would be wrong in one of them.
   */
  it('says the session toggle applies to this chat, and where the default lives', async () => {
    render(<BottomMenuExtensionSelection sessionId="session-1" />);
    fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
      button: 0,
      ctrlKey: false,
    });
    await screen.findAllByRole('menuitemcheckbox');

    expect(screen.getByText(/Applies to this chat\./)).toBeInTheDocument();
    expect(screen.getByText(/Settings → Extensions/)).toBeInTheDocument();
  });

  it('says the hub toggle applies to the next chat, not to "new chats"', async () => {
    render(<BottomMenuExtensionSelection sessionId={null} />);
    fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
      button: 0,
      ctrlKey: false,
    });
    await screen.findAllByRole('menuitemcheckbox');

    // `clearExtensionOverrides` runs on session creation, so the override shapes
    // exactly one chat — "in new chats" was an over-promise.
    expect(screen.getByText(/Applies to the next chat you start\./)).toBeInTheDocument();
    expect(screen.queryByText(/Applies to this chat\./)).not.toBeInTheDocument();
  });

  it('keeps shipped capabilities out of the per-chat extension selector', async () => {
    render(<BottomMenuExtensionSelection sessionId={null} />);
    const trigger = screen.getByLabelText('Manage extensions (2 enabled)');
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });

    expect(await screen.findAllByRole('menuitemcheckbox')).toHaveLength(1);
    expect(screen.getByText('example')).toBeInTheDocument();
    expect(screen.queryByText('autovisualiser')).not.toBeInTheDocument();
    expect(screen.queryByText('code_execution')).not.toBeInTheDocument();
    expect(screen.queryByText('chatrecall')).not.toBeInTheDocument();
    expect(screen.queryByText('agent_drafter')).not.toBeInTheDocument();
  });

  it('serializes rapid session toggles so the latest choice reaches the backend last', async () => {
    let resolveEnable: (() => void) | undefined;
    mocks.addToAgent.mockImplementationOnce(
      () => new Promise<void>((resolve) => (resolveEnable = resolve))
    );
    render(<BottomMenuExtensionSelection sessionId="session-1" />);
    fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
      button: 0,
      ctrlKey: false,
    });

    const toggle = await screen.findByRole('menuitemcheckbox');
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));

    expect(mocks.addToAgent).toHaveBeenCalledTimes(1);
    expect(mocks.removeFromAgent).not.toHaveBeenCalled();
    resolveEnable?.();

    await waitFor(() => expect(mocks.removeFromAgent).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
  });
});
