import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PrivacyPanel, { DISABLE_PHRASE, PRIVACY_TIERS_KEY } from './PrivacyPanel';

const mocks = vi.hoisted(() => ({
  value: undefined as unknown,
  read: vi.fn(),
  upsert: vi.fn(),
}));

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({ read: mocks.read, upsert: mocks.upsert }),
}));

describe('Settings > Privacy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.value = undefined;
    mocks.read.mockImplementation(async () => mocks.value);
    mocks.upsert.mockImplementation(async () => undefined);
  });

  it('defaults to on when the key is absent', async () => {
    render(<PrivacyPanel />);
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: /Privacy tiers/ })).toBeChecked()
    );
  });

  it('does not turn off on a single click — it asks for the phrase first', async () => {
    const user = userEvent.setup();
    render(<PrivacyPanel />);
    await waitFor(() => screen.getByRole('switch', { name: /Privacy tiers/ }));

    await user.click(screen.getByRole('switch', { name: /Privacy tiers/ }));
    expect(mocks.upsert).not.toHaveBeenCalled();

    // All four sentences, and the two a user cannot reconstruct for themselves.
    const dialog = screen.getByTestId('privacy-disable-confirm');
    expect(dialog).toHaveTextContent(/every.*privacy guardrail on this machine/i);
    expect(dialog).toHaveTextContent(/read and write your knowledge bases/i);
    expect(dialog).toHaveTextContent(/stops recording which conversations touched private/i);
    expect(dialog).toHaveTextContent(/cannot go back and mark anything that happened/i);
  });

  it('compares the phrase exactly, then writes with the confirmation', async () => {
    const user = userEvent.setup();
    render(<PrivacyPanel />);
    await waitFor(() => screen.getByRole('switch', { name: /Privacy tiers/ }));
    await user.click(screen.getByRole('switch', { name: /Privacy tiers/ }));

    const field = screen.getByLabelText('Confirmation phrase');
    const button = screen.getByRole('button', { name: /Turn off privacy tiers/ });

    // Lower case is NOT the phrase. This is the assertion that fails a
    // `toLowerCase()` or a `trim()` creeping into the comparison.
    await user.type(field, DISABLE_PHRASE.toLowerCase());
    expect(button).toBeDisabled();

    await user.clear(field);
    await user.type(field, DISABLE_PHRASE);
    expect(button).toBeEnabled();

    await user.click(button);
    await waitFor(() =>
      expect(mocks.upsert).toHaveBeenCalledWith(PRIVACY_TIERS_KEY, 'off', false, DISABLE_PHRASE)
    );
  });

  it('shows the persistent strip while enforcement is off, and turning it back on needs no phrase', async () => {
    const user = userEvent.setup();
    mocks.value = 'off';
    render(<PrivacyPanel />);

    await waitFor(() =>
      expect(screen.getByRole('switch', { name: /Privacy tiers/ })).not.toBeChecked()
    );
    expect(screen.getByTestId('privacy-enforcement-off-strip')).toHaveTextContent(
      /Privacy tiers are off/i
    );

    await user.click(screen.getByRole('switch', { name: /Privacy tiers/ }));
    await waitFor(() =>
      expect(mocks.upsert).toHaveBeenCalledWith(PRIVACY_TIERS_KEY, 'on', false, DISABLE_PHRASE)
    );
  });

  it('a refused write leaves the switch showing what is true, not what was asked', async () => {
    const user = userEvent.setup();
    mocks.upsert.mockRejectedValue(new Error('403 Forbidden'));
    render(<PrivacyPanel />);
    await waitFor(() => screen.getByRole('switch', { name: /Privacy tiers/ }));

    await user.click(screen.getByRole('switch', { name: /Privacy tiers/ }));
    await user.type(screen.getByLabelText('Confirmation phrase'), DISABLE_PHRASE);
    await user.click(screen.getByRole('button', { name: /Turn off privacy tiers/ }));

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('403'));
    expect(screen.getByRole('switch', { name: /Privacy tiers/ })).toBeChecked();
  });
});
