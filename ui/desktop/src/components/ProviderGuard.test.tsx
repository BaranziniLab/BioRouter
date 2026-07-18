import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProviderGuard from './ProviderGuard';

const mocks = vi.hoisted(() => ({
  read: vi.fn(),
  upsert: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock('./ConfigContext', () => ({
  useConfig: () => ({ read: mocks.read, upsert: mocks.upsert }),
}));

vi.mock('react-router-dom', () => ({
  useNavigate: () => mocks.navigate,
}));

vi.mock('./onboarding/LlamaServerInlineCard', () => ({ default: () => null }));
vi.mock('./onboarding/OllamaInlineCard', () => ({ default: () => null }));
vi.mock('./onboarding/InstitutionalSetupCard', () => ({ default: () => null }));
vi.mock('./onboarding/CommercialSetupCard', () => ({
  default: ({ onSuccess }: { onSuccess: (setup: unknown) => Promise<void> }) => (
    <button
      onClick={() =>
        void onSuccess({
          provider: 'xiaomi_mimo',
          model: 'mimo-v2-flash',
          models: ['mimo-v2-flash'],
          apiKey: 'mimo-secret',
          apiKeyConfigKey: 'XIAOMI_MIMO_API_KEY',
          extraConfig: { XIAOMI_MIMO_HOST: 'https://api.xiaomimimo.com' },
        })
      }
    >
      Complete detection
    </button>
  ),
}));

vi.mock('./settings/models/subcomponents/SwitchModelModal', () => ({
  SwitchModelModal: ({
    initialProvider,
    initialModel,
  }: {
    initialProvider?: string | null;
    initialModel?: string | null;
  }) => (
    <div>
      <span>Provider: {initialProvider}</span>
      <span>Model: {initialModel}</span>
    </div>
  ),
}));

describe('ProviderGuard commercial onboarding', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.read.mockResolvedValue('');
    mocks.upsert.mockResolvedValue(undefined);
  });

  it('persists the detected provider contract before opening its model picker', async () => {
    render(
      <ProviderGuard didSelectProvider={false}>
        <div>Application</div>
      </ProviderGuard>
    );

    // The brand mark renders in both the checking loader and the onboarding
    // header; assert it appears without re-checking document attachment, since
    // the loader's mark detaches the instant `isChecking` flips to the header.
    expect(await screen.findByRole('img', { name: 'BioRouter' })).toBeTruthy();
    fireEvent.click(await screen.findByRole('button', { name: 'Complete detection' }));

    await waitFor(() => {
      expect(mocks.upsert.mock.calls).toEqual([
        ['XIAOMI_MIMO_API_KEY', 'mimo-secret', true],
        ['XIAOMI_MIMO_HOST', 'https://api.xiaomimimo.com', false],
        ['BIOROUTER_PROVIDER', 'xiaomi_mimo', false],
      ]);
      expect(screen.getByText('Provider: xiaomi_mimo')).toBeInTheDocument();
      expect(screen.getByText('Model: mimo-v2-flash')).toBeInTheDocument();
    });
  });
});
