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

  // The App tab is a stack of titled `biorouter-settings-section` blocks, each
  // with a header and a `biorouter-settings-list`. A bare row mounted after
  // `AppSettingsSection`'s `pb-8` renders — the row class carries its own
  // border and hover — but as an unlabelled orphan under the previous
  // section's heading, so the user reads it as part of "Updates".
  it('renders as a titled section, not a headerless orphan row', async () => {
    mocks.read.mockResolvedValue(false);
    const { container } = render(<WorkspaceSettingsSection />);
    await screen.findByRole('switch', { name: /never open tabs automatically/i });

    expect(screen.getByRole('heading', { name: /workspace/i })).toBeInTheDocument();
    const section = container.querySelector('.biorouter-settings-section');
    expect(section).not.toBeNull();
    expect(
      section?.querySelector('.biorouter-settings-list .biorouter-settings-row')
    ).not.toBeNull();
  });
});
