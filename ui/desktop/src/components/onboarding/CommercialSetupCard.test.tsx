import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import CommercialSetupCard from './CommercialSetupCard';

const mockDetectProvider = vi.fn();
const mockGetDetectableProviders = vi.fn();

vi.mock('../../api', () => ({
  detectProvider: (...args: unknown[]) => mockDetectProvider(...args),
  getDetectableProviders: (...args: unknown[]) => mockGetDetectableProviders(...args),
}));

vi.mock('react-router-dom', () => ({
  useNavigate: () => vi.fn(),
}));

function typeKeyAndSubmit(value: string) {
  const input = screen.getByPlaceholderText('Paste your API key here…');
  fireEvent.change(input, { target: { value } });
  // The arrow button is the only button in the input row.
  const buttons = screen.getAllByRole('button');
  fireEvent.click(buttons[0]);
}

describe('CommercialSetupCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetDetectableProviders.mockResolvedValue({
      data: {
        providers: [
          { name: 'openai', display_name: 'OpenAI' },
          { name: 'anthropic', display_name: 'Anthropic' },
        ],
      },
    });
  });

  it('renders the supported-provider list fetched from the backend', async () => {
    render(<CommercialSetupCard onSuccess={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText(/Paste a key from OpenAI, Anthropic/)).toBeInTheDocument();
    });
  });

  it('calls onSuccess with provider, default model, key and extra config on success', async () => {
    mockDetectProvider.mockResolvedValue({
      data: {
        provider_name: 'openai',
        api_key_config_key: 'OPENAI_API_KEY',
        models: ['gpt-4o', 'gpt-4o-mini'],
        default_model: 'gpt-4o',
        extra_config: { FOO_HOST: 'https://example.com' },
        reason: null,
      },
    });
    const onSuccess = vi.fn();
    render(<CommercialSetupCard onSuccess={onSuccess} />);

    typeKeyAndSubmit('sk-test-key');

    await waitFor(() => expect(screen.getByText('Detected openai')).toBeInTheDocument());
    await waitFor(() =>
      expect(onSuccess).toHaveBeenCalledWith({
        provider: 'openai',
        model: 'gpt-4o',
        models: ['gpt-4o', 'gpt-4o-mini'],
        apiKey: 'sk-test-key',
        apiKeyConfigKey: 'OPENAI_API_KEY',
        extraConfig: { FOO_HOST: 'https://example.com' },
      })
    );
  });

  it.each([
    ['anthropic', 'ANTHROPIC_API_KEY', 'sk-ant-test'],
    ['google', 'GOOGLE_API_KEY', 'AIza-test'],
    ['groq', 'GROQ_API_KEY', 'gsk_test'],
    ['xai', 'XAI_API_KEY', 'xai-test'],
    ['openai', 'OPENAI_API_KEY', 'sk-openai-test'],
    ['zai', 'ZAI_API_KEY', 'zai-test'],
    ['xiaomi_mimo', 'XIAOMI_MIMO_API_KEY', 'mimo-test'],
  ])('hands off the exact %s secret configuration key', async (provider, configKey, key) => {
    mockDetectProvider.mockResolvedValue({
      data: {
        provider_name: provider,
        api_key_config_key: configKey,
        models: ['model-1'],
        default_model: 'model-1',
        extra_config: {},
        reason: null,
      },
    });
    const onSuccess = vi.fn();
    render(<CommercialSetupCard onSuccess={onSuccess} />);

    typeKeyAndSubmit(`  ${key}  `);

    await waitFor(() =>
      expect(onSuccess).toHaveBeenCalledWith(
        expect.objectContaining({
          provider,
          apiKeyConfigKey: configKey,
          apiKey: key,
          model: 'model-1',
        })
      )
    );
    expect(mockDetectProvider).toHaveBeenCalledWith({ body: { api_key: key } });
  });

  it('keeps onboarding open with an actionable error when saving fails', async () => {
    mockDetectProvider.mockResolvedValue({
      data: {
        provider_name: 'openai',
        api_key_config_key: 'OPENAI_API_KEY',
        models: ['gpt-4o'],
        default_model: 'gpt-4o',
        extra_config: {},
        reason: null,
      },
    });
    render(<CommercialSetupCard onSuccess={vi.fn().mockRejectedValue(new Error('disk full'))} />);

    typeKeyAndSubmit('sk-test');

    await waitFor(() => expect(screen.getByText('Could not save provider')).toBeInTheDocument());
    expect(screen.queryByText('Detected openai')).not.toBeInTheDocument();
  });

  it('shows the invalid-key message when the provider rejects the key', async () => {
    mockDetectProvider.mockResolvedValue({
      data: { provider_name: null, models: [], default_model: null, reason: 'invalid_key' },
    });
    const onSuccess = vi.fn();
    render(<CommercialSetupCard onSuccess={onSuccess} />);

    typeKeyAndSubmit('sk-ant-bad');

    await waitFor(() => expect(screen.getByText('Key was rejected')).toBeInTheDocument());
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it('shows the no-match message when nothing validates', async () => {
    mockDetectProvider.mockResolvedValue({
      data: { provider_name: null, models: [], default_model: null, reason: 'no_match' },
    });
    render(<CommercialSetupCard onSuccess={vi.fn()} />);

    typeKeyAndSubmit('unknown-key');

    await waitFor(() => expect(screen.getByText('Could not detect provider')).toBeInTheDocument());
  });

  it('treats a thrown transport error as a network failure', async () => {
    mockDetectProvider.mockRejectedValue(new Error('boom'));
    render(<CommercialSetupCard onSuccess={vi.fn()} />);

    typeKeyAndSubmit('sk-test');

    await waitFor(() =>
      expect(screen.getByText('Could not reach the provider')).toBeInTheDocument()
    );
  });
});
