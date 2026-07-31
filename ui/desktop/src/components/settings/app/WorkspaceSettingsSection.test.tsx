import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

const mocks = vi.hoisted(() => ({ upsert: vi.fn(), read: vi.fn() }));

vi.mock('../../ConfigContext', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return {
    ...actual,
    useConfig: () => ({ upsert: mocks.upsert, read: mocks.read }),
  };
});

import { WorkspaceSettingsSection } from './WorkspaceSettingsSection';

describe('WorkspaceSettingsSection', () => {
  afterEach(() => vi.clearAllMocks());

  it('reflects the stored value and writes the config key on toggle', async () => {
    mocks.read.mockResolvedValue(false);
    render(<WorkspaceSettingsSection />);
    const toggle = await screen.findByRole('switch', { name: /never open tabs automatically/i });
    expect(toggle.getAttribute('aria-checked')).toBe('false');

    fireEvent.click(toggle);
    await waitFor(() =>
      expect(mocks.upsert).toHaveBeenCalledWith('WORKSPACE_ANNOUNCE_ONLY', true, false)
    );
  });

  it('starts checked when the key is already true', async () => {
    mocks.read.mockResolvedValue(true);
    render(<WorkspaceSettingsSection />);
    const toggle = await screen.findByRole('switch', { name: /never open tabs automatically/i });
    await waitFor(() => expect(toggle.getAttribute('aria-checked')).toBe('true'));
  });
});
