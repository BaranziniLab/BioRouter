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
        name: 'example',
        display_name: 'Example',
        description: 'Example extension',
        enabled: false,
      },
    ],
  }),
}));

vi.mock('../settings/capabilities/capabilities', () => ({
  isCapabilityExtension: () => false,
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
  });

  it('keeps an immediate hub toggle when the menu closes and reopens', async () => {
    render(<BottomMenuExtensionSelection sessionId={null} />);
    const trigger = screen.getByTitle('manage extensions');
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

  it('serializes rapid session toggles so the latest choice reaches the backend last', async () => {
    let resolveEnable: (() => void) | undefined;
    mocks.addToAgent.mockImplementationOnce(
      () => new Promise<void>((resolve) => (resolveEnable = resolve))
    );
    render(<BottomMenuExtensionSelection sessionId="session-1" />);
    fireEvent.pointerDown(screen.getByTitle('manage extensions'), { button: 0, ctrlKey: false });

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
