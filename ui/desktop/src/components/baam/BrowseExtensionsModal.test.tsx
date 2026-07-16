import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import BrowseExtensionsModal from './BrowseExtensionsModal';

const loadRegistry = vi.hoisted(() => vi.fn());

vi.mock('./registry', () => ({
  loadRegistry,
  extensionMatches: () => true,
}));

vi.mock('../BrxtInstallModal', async () => {
  const { Dialog, DialogContent, DialogTitle } = await import('../ui/dialog');
  return {
    BrxtInstallModal: ({ onClose }: { onClose: () => void }) => (
      <Dialog open onOpenChange={(open) => !open && onClose()}>
        <DialogContent>
          <DialogTitle>Configure downloaded extension</DialogTitle>
        </DialogContent>
      </Dialog>
    ),
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
});
