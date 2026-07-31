import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { resetChatrecallSuggestionForTests } from './chatrecallSuggestion';

const mocks = vi.hoisted(() => ({
  extensionsList: [] as Array<Record<string, unknown>>,
  addExtension: vi.fn(async () => undefined),
  removeExtension: vi.fn(async () => undefined),
  getExtensions: vi.fn(async () => []),
  toggleExtensionDefault: vi.fn(async () => undefined),
  activateExtensionDefault: vi.fn(async () => undefined),
  deleteExtension: vi.fn(async () => undefined),
  success: vi.fn(),
  error: vi.fn(),
}));

// `useConfig` THROWS outside a ConfigProvider (`ConfigContext.tsx`, the
// `throw new Error('useConfig must be used within a ConfigProvider')`) and the
// context object itself is module-private (`const ConfigContext =
// createContext<ConfigContextType | undefined>(undefined);`, no `export`), so it
// cannot be wrapped — the component must be given a mocked hook. Same shape as
// the sibling `capabilities/CapabilitiesSection.test.tsx`.
vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: mocks.extensionsList,
    addExtension: mocks.addExtension,
    removeExtension: mocks.removeExtension,
    getExtensions: mocks.getExtensions,
  }),
}));

// `./index` re-exports the real extension-manager calls, which would hit the daemon.
vi.mock('./index', () => ({
  toggleExtensionDefault: mocks.toggleExtensionDefault,
  activateExtensionDefault: mocks.activateExtensionDefault,
  deleteExtension: mocks.deleteExtension,
}));

vi.mock('../../../toasts', () => ({
  toastService: { success: mocks.success, error: mocks.error },
}));

// DEFAULT export — `ExtensionsSection.tsx` is `export default function
// ExtensionsSection(...)`; there is no named export, so `import { ExtensionsSection }`
// is `undefined` and React throws "type is invalid" at render.
import ExtensionsSection from './ExtensionsSection';

// `ExtensionItem` labels its Radix switch `Toggle ${getFriendlyTitle(extension)}
// extension` (`subcomponents/ExtensionItem.tsx`), and `getFriendlyTitle` for a
// `platform` entry named `workspace` is `formatExtensionName('workspace')` =
// 'Workspace' (it is not in PLATFORM_EXTENSION_DISPLAY_NAMES).
const WORKSPACE_SWITCH = 'Toggle Workspace extension';

describe('ExtensionsSection chatrecall suggestion', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetChatrecallSuggestionForTests();
    mocks.extensionsList = [
      { type: 'platform', name: 'workspace', description: 'Workspace Control', enabled: false },
      { type: 'platform', name: 'chatrecall', description: 'Recall chats', enabled: false },
    ];
  });

  it('suggests chatrecall once when Workspace Control is switched on', async () => {
    render(<ExtensionsSection hideButtons />);

    const toggle = await screen.findByRole('switch', { name: WORKSPACE_SWITCH });
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.success).toHaveBeenCalledTimes(1));
    expect(mocks.toggleExtensionDefault).toHaveBeenCalledWith(
      expect.objectContaining({ toggle: 'toggleOn' })
    );

    // …and never again. `ExtensionItem` disables its switch for the duration of
    // the in-flight toggle (`isToggling`), so each further click must wait for
    // the previous one to settle — clicking three times in a row would fire
    // exactly one event and prove nothing.
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.toggleExtensionDefault).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.toggleExtensionDefault).toHaveBeenCalledTimes(3));

    expect(mocks.success).toHaveBeenCalledTimes(1);
  });

  it('does not suggest chatrecall when it is already enabled', async () => {
    mocks.extensionsList = [
      { type: 'platform', name: 'workspace', description: 'Workspace Control', enabled: false },
      { type: 'platform', name: 'chatrecall', description: 'Recall chats', enabled: true },
    ];
    render(<ExtensionsSection hideButtons />);

    fireEvent.click(await screen.findByRole('switch', { name: WORKSPACE_SWITCH }));
    await waitFor(() => expect(mocks.toggleExtensionDefault).toHaveBeenCalledTimes(1));
    expect(mocks.success).not.toHaveBeenCalled();
  });

  it('does not suggest chatrecall when Workspace Control is switched OFF', async () => {
    mocks.extensionsList = [
      { type: 'platform', name: 'workspace', description: 'Workspace Control', enabled: true },
      { type: 'platform', name: 'chatrecall', description: 'Recall chats', enabled: false },
    ];
    render(<ExtensionsSection hideButtons />);

    fireEvent.click(await screen.findByRole('switch', { name: WORKSPACE_SWITCH }));
    await waitFor(() =>
      expect(mocks.toggleExtensionDefault).toHaveBeenCalledWith(
        expect.objectContaining({ toggle: 'toggleOff' })
      )
    );
    expect(mocks.success).not.toHaveBeenCalled();
  });
});
