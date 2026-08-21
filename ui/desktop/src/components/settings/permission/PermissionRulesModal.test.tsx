import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PermissionRulesModal from './PermissionRulesModal';

const { getExtensions } = vi.hoisted(() => ({ getExtensions: vi.fn() }));

vi.mock('../../ConfigContext', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../ConfigContext')>();
  return { ...actual, useConfig: () => ({ getExtensions }) };
});

vi.mock('./PermissionModal', () => ({
  default: ({ extensionName }: { extensionName: string }) => (
    <div data-testid="permission-modal">{extensionName}</div>
  ),
}));

describe('PermissionRulesModal', () => {
  beforeEach(() => {
    getExtensions.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lists enabled real extensions without fabricating a platform extension', async () => {
    getExtensions.mockResolvedValue([
      {
        type: 'builtin',
        name: 'developer',
        display_name: 'Developer',
        description: 'A very long description that must remain inside the permission row.',
        enabled: true,
      },
      {
        type: 'builtin',
        name: 'platform',
        display_name: 'Platform',
        description: 'Synthetic internal grouping',
        enabled: true,
      },
      {
        type: 'builtin',
        name: 'disabled',
        display_name: 'Disabled',
        description: 'Disabled extension',
        enabled: false,
      },
    ]);

    render(<PermissionRulesModal isOpen onClose={vi.fn()} />);

    const developer = await screen.findByRole('button', { name: /Developer/ });
    expect(developer).toHaveClass('whitespace-normal');
    expect(screen.queryByText('Synthetic internal grouping')).not.toBeInTheDocument();
    expect(screen.queryByText('Disabled extension')).not.toBeInTheDocument();

    fireEvent.click(developer);
    expect(screen.getByTestId('permission-modal')).toHaveTextContent('developer');
  });

  it('shows a retryable error instead of leaving the extension list loading forever', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    getExtensions.mockRejectedValue(new Error('unavailable'));

    render(<PermissionRulesModal isOpen onClose={vi.fn()} />);

    expect(await screen.findByText('Enabled extensions could not be loaded.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(error).toHaveBeenCalledWith(
      'Failed to load extensions for permission settings:',
      expect.objectContaining({ message: 'unavailable' })
    );
  });
});
