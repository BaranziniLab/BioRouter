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
    await waitFor(
      () =>
        expect(onSuccess).toHaveBeenCalledWith('openai', 'gpt-4o', 'sk-test-key', {
          FOO_HOST: 'https://example.com',
        }),
      { timeout: 2500 }
    );
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
