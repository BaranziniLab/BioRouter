import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import InstitutionalSetupCard from './InstitutionalSetupCard';

const mockUpsert = vi.fn();
const mockCheckProvider = vi.fn();

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ upsert: (...args: unknown[]) => mockUpsert(...args) }),
}));
vi.mock('../../api', () => ({
  checkProvider: (...args: unknown[]) => mockCheckProvider(...args),
}));
vi.mock('react-router-dom', () => ({ useNavigate: () => vi.fn() }));

/** The three keys that belong to the PUBLIC `azure_openai` provider. */
const PUBLIC_AZURE_KEYS = [
  'AZURE_OPENAI_ENDPOINT',
  'AZURE_OPENAI_DEPLOYMENT_NAME',
  'AZURE_OPENAI_API_VERSION',
];

async function connectVersaAzure() {
  render(<InstitutionalSetupCard onSuccess={vi.fn()} />);
  fireEvent.change(screen.getByLabelText(/API Key/i), { target: { value: 'a-key' } });
  fireEvent.click(screen.getByRole('button', { name: /Connect to Versa Azure OpenAI/i }));
  await waitFor(() => expect(mockCheckProvider).toHaveBeenCalled());
}

describe('InstitutionalSetupCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUpsert.mockResolvedValue(undefined);
    mockCheckProvider.mockResolvedValue({ data: {} });
  });

  it('never writes the public Azure provider keys when connecting UCSF Versa', async () => {
    // Connecting UCSF's PRIVATE Versa used to write the PUBLIC `azure_openai`
    // card's own three keys, with values identical to the defaults Versa
    // already falls back to. The write changed nothing for Versa and made
    // `check_provider_configured` report a Public provider the user never set
    // up as Configured, one row away in the same grid.
    await connectVersaAzure();
    const written = mockUpsert.mock.calls.map((c) => c[0] as string);
    for (const key of PUBLIC_AZURE_KEYS) {
      expect(written).not.toContain(key);
    }
  });

  it('writes the key and the Versa-namespaced overrides', async () => {
    await connectVersaAzure();
    const written = mockUpsert.mock.calls.map((c) => c[0] as string);
    expect(written).toContain('VERSA_AZURE_API_KEY');
    expect(written).toContain('VERSA_AZURE_ENDPOINT');
    expect(written).toContain('VERSA_AZURE_DEPLOYMENT_NAME');
    expect(written).toContain('VERSA_AZURE_API_VERSION');
  });
});
