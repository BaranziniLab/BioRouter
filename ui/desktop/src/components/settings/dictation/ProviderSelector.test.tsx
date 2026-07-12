import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ProviderSelector } from './ProviderSelector';

const getProviders = vi.fn();

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({ getProviders }),
}));

beforeEach(() => {
  getProviders.mockReset();
  getProviders.mockResolvedValue([{ name: 'openai', is_configured: true }]);
});

describe('ProviderSelector', () => {
  it('dismisses the provider menu with Escape and restores trigger focus', async () => {
    const user = userEvent.setup();
    render(
      <ProviderSelector
        settings={{ enabled: true, provider: 'openai' }}
        onProviderChange={vi.fn()}
      />
    );

    const trigger = screen.getByRole('button', { name: 'Choose dictation provider' });
    await user.click(trigger);
    expect(screen.getByRole('menu')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it('dismisses on outside click and selects from keyboard-aware radio items', async () => {
    const user = userEvent.setup();
    const onProviderChange = vi.fn();
    render(
      <>
        <ProviderSelector
          settings={{ enabled: true, provider: null }}
          onProviderChange={onProviderChange}
        />
        <button type="button">Outside</button>
      </>
    );

    const trigger = screen.getByRole('button', { name: 'Choose dictation provider' });
    await user.click(trigger);
    await user.click(screen.getByRole('menuitemradio', { name: 'OpenAI Whisper' }));
    expect(onProviderChange).toHaveBeenCalledWith('openai');

    await user.click(trigger);
    await user.click(screen.getByRole('button', { name: 'Outside' }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});
