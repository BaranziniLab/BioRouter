import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import BrowseExtensionsModal from './BrowseExtensionsModal';

const loadRegistry = vi.hoisted(() => vi.fn());

// Spread the real module rather than listing members: a partial factory means
// every export this component newly reaches for (`effectivePrivacy`,
// `catalogFreshnessLine`) arrives `undefined` and the modal dies at render, in a
// test that has nothing to say about either. Only the two seams the test
// actually controls are replaced.
vi.mock('./registry', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./registry')>()),
  loadRegistry,
  extensionMatches: () => true,
}));

const seenRegistrySource = vi.hoisted(() => vi.fn());

vi.mock('../BrxtInstallModal', async () => {
  const { Dialog, DialogContent, DialogTitle } = await import('../ui/dialog');
  return {
    BrxtInstallModal: ({
      onClose,
      registrySource,
    }: {
      onClose: () => void;
      registrySource?: { registryId: string; sourceUrl?: string };
    }) => {
      seenRegistrySource(registrySource);
      return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
          <DialogContent aria-describedby={undefined}>
            <DialogTitle>Configure downloaded extension</DialogTitle>
          </DialogContent>
        </Dialog>
      );
    },
  };
});

afterEach(cleanup);

beforeEach(() => {
  loadRegistry.mockResolvedValue({
    live: true,
    registry: {
      extensions: [
        {
          id: 'test-extension',
          name: 'Test Extension',
          organization: 'Biorouter',
          version: '1.0.0',
          description: 'Test extension',
          tags: [],
          download: 'https://example.test/test.brxt',
        },
      ],
      skills: [],
    },
  });
  (window as unknown as { electron: unknown }).electron = {
    downloadRegistryAsset: vi.fn().mockResolvedValue({ path: '/tmp/test.brxt' }),
  };
});

describe('BrowseExtensionsModal', () => {
  it('returns from extension configuration without closing the marketplace', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(
      <BrowseExtensionsModal onClose={onClose} onInstalled={() => {}} installedNames={new Set()} />
    );

    await user.click(await screen.findByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Configure downloaded extension')).toBeInTheDocument();

    await user.keyboard('{Escape}');

    await waitFor(() => expect(screen.getByText('Browse Extensions')).toBeInTheDocument());
    expect(onClose).not.toHaveBeenCalled();
  });

  /**
   * Issue #56 Task 43 (DR-23). The registry `id` exists only here, and the
   * install that has to record it happens one component away — so the handoff
   * is the whole mechanism. Without it every marketplace install records no
   * provenance and a renamed private extension classifies public again.
   */
  it('hands the install the registry id and download URL the bundle came from', async () => {
    const user = userEvent.setup();
    seenRegistrySource.mockClear();

    render(
      <BrowseExtensionsModal onClose={() => {}} onInstalled={() => {}} installedNames={new Set()} />
    );

    await user.click(await screen.findByRole('button', { name: 'Add' }));
    await screen.findByText('Configure downloaded extension');

    expect(seenRegistrySource).toHaveBeenCalledWith({
      registryId: 'test-extension',
      sourceUrl: 'https://example.test/test.brxt',
    });
  });
});
