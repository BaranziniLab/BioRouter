import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PermissionModal from './PermissionModal';

const { getTools, upsertPermissions } = vi.hoisted(() => ({
  getTools: vi.fn(),
  upsertPermissions: vi.fn(),
}));

vi.mock('../../../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../api')>();
  return { ...actual, getTools, upsertPermissions };
});

describe('PermissionModal', () => {
  beforeEach(() => {
    getTools.mockReset();
    upsertPermissions.mockReset();
  });

  it('loads the configured extension independently of an active chat', async () => {
    getTools.mockResolvedValue({
      data: [
        {
          name: 'extensionmanager__search_available_extensions',
          description: 'Find extensions that can help with a task',
          parameters: [],
          permission: 'ask_before',
        },
      ],
    });

    render(
      <PermissionModal
        extensionName="Extension Manager"
        extensionLabel="Extension Manager"
        onClose={vi.fn()}
      />
    );

    expect(await screen.findByText('Search Available Extensions')).toBeInTheDocument();
    expect(screen.getByText('Find extensions that can help with a task')).toBeInTheDocument();
    expect(getTools).toHaveBeenCalledWith({
      query: { extension_name: 'Extension Manager', session_id: '' },
    });
  });

  it('shows a completed empty state instead of an endless loading indicator', async () => {
    getTools.mockResolvedValue({ data: [] });

    render(<PermissionModal extensionName="empty" onClose={vi.fn()} />);

    expect(await screen.findByText('No configurable tools')).toBeInTheDocument();
    expect(screen.queryByText('Loading tools…')).not.toBeInTheDocument();
  });

  it('shows an actionable error state and can retry', async () => {
    getTools
      .mockResolvedValueOnce({ error: { message: 'failed' } })
      .mockResolvedValueOnce({ data: [] });

    render(<PermissionModal extensionName="broken" onClose={vi.fn()} />);

    expect(await screen.findByText('Tools could not be loaded')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));

    await waitFor(() => expect(getTools).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('No configurable tools')).toBeInTheDocument();
  });
});
