import { fireEvent, render, screen } from '@testing-library/react';
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
        type: 'stdio',
        name: 'ucsfomopagent',
        display_name: 'UCSFOMOPAgent',
        description: 'UCSF OMOP clinical database',
        cmd: 'uv',
        args: [],
        enabled: true,
      },
      {
        type: 'stdio',
        name: 'example',
        display_name: 'Example',
        description: 'Example extension',
        cmd: 'example',
        args: [],
        enabled: true,
      },
    ],
  }),
}));

vi.mock('../settings/extensions/subcomponents/ExtensionList', () => ({
  formatExtensionName: (name: string) => name,
  isBuiltInExtension: () => false,
}));

vi.mock('../../api', () => ({ getSessionExtensions: mocks.getSessionExtensions }));

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

/**
 * ⚠ Radix writes `aria-disabled={disabled || undefined}` on a menu item, so an
 * ENABLED row carries no such attribute at all — `toHaveAttribute('aria-disabled',
 * 'false')` fails against a perfectly good row. The negative assertions below
 * therefore read "not disabled", which is what the DOM can actually express.
 */
async function openMenu() {
  fireEvent.pointerDown(screen.getByLabelText(/Manage extensions/), {
    button: 0,
    ctrlKey: false,
  });
  await screen.findAllByRole('menuitemcheckbox');
}

describe('BottomMenuExtensionSelection — the pairing, not the extension', () => {
  beforeEach(() => {
    mocks.overrides.clear();
    vi.clearAllMocks();
  });

  it('a private extension is visible-but-disabled in the composer, never omitted', async () => {
    // Omission is what produces "the OMOP tool is broken". Gate C is invisible
    // in the GUI by construction — it returns ErrorData from inside
    // dispatch_tool_call, so it never enters PermissionCheckResult and produces
    // no approval card and no denial record.
    //
    // ⚠ VISIBLE is asserted first and on its own, because it is the half that
    // the task's `grep -c "aria-disabled"` gate cannot see. Radix derives that
    // attribute from `disabled`, so the literal appears nowhere in the JSX and
    // the grep is satisfied by the explanatory comment beside it — an
    // implementation that FILTERED the row out and kept the comment would score
    // green. This assertion is the real tripwire for that wrong turn: the
    // refused row must still be in the list, and the list must still be whole.
    render(<BottomMenuExtensionSelection sessionId="s1" privacyTier="public" />);
    await openMenu();

    expect(screen.getAllByRole('menuitemcheckbox')).toHaveLength(2);
    expect(screen.getByText('ucsfomopagent')).toBeInTheDocument();

    const item = screen.getByText('ucsfomopagent').closest('[role="menuitemcheckbox"]')!;
    expect(item).toHaveAttribute('aria-disabled', 'true');
    expect(item).toHaveTextContent(/public model/i);
  });

  it('leaves every other extension alone in the same chat', async () => {
    render(<BottomMenuExtensionSelection sessionId="s1" privacyTier="public" />);
    await openMenu();

    const item = screen.getByText('example').closest('[role="menuitemcheckbox"]')!;
    expect(item).not.toHaveAttribute('aria-disabled', 'true');
    expect(item).not.toHaveTextContent(/public model/i);
  });

  it('leaves the private extension usable once the chat is private', async () => {
    render(<BottomMenuExtensionSelection sessionId="s1" privacyTier="private" />);
    await openMenu();

    const item = screen.getByText('ucsfomopagent').closest('[role="menuitemcheckbox"]')!;
    expect(item).not.toHaveAttribute('aria-disabled', 'true');
    expect(item).not.toHaveTextContent(/public model/i);
  });

  // A tier nobody could resolve is not a claim that the chat is public. Walling
  // a working tool on a missing read is the failure this whole state exists to
  // prevent.
  it('judges nothing when the chat tier is unknown', async () => {
    render(<BottomMenuExtensionSelection sessionId="s1" />);
    await openMenu();

    const item = screen.getByText('ucsfomopagent').closest('[role="menuitemcheckbox"]')!;
    expect(item).not.toHaveAttribute('aria-disabled', 'true');
  });
});
