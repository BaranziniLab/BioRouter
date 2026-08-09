import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProviderDetails, ProviderTier } from '../../../api';
import ProviderGrid from './ProviderGrid';
import { __resetDisclosureStoreForTests } from '../../privacy/disclosureCopy';

// Task 30A: the Commercial section carries the served one-line disclosure.
// ⚠ The fixture is deliberately not the product's sentence — Step 5's gate (1)
// counts definitions of that sentence across `ui/desktop/src/` and expects one,
// and a `--include='*.tsx'` grep does not skip test files.
const mocks = vi.hoisted(() => ({
  getPrivacyDisclosure: vi.fn(),
  ackPrivacyDisclosure: vi.fn(),
}));

vi.mock('../../../api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getPrivacyDisclosure: mocks.getPrivacyDisclosure,
  ackPrivacyDisclosure: mocks.ackPrivacyDisclosure,
}));
vi.mock('../../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

/**
 * §14.5 — the two taxonomies must be the same words in the same place.
 *
 * ⚠ These assertions live here, not in `providerOrdering.test.ts`, and that is
 * the whole point of this file. `ProviderGrid` imports
 * `getOrderedProviderGroups` for its *ordering* and then ignores `label`
 * entirely, printing three hardcoded literals of its own. Relabelling the data
 * alone changes nothing a user can see, and a unit test of the data alone would
 * be green while the screen still said "Institutional Models".
 */
function provider(
  name: string,
  backend: { tier?: ProviderTier; runs_locally?: boolean } = {}
): ProviderDetails {
  return {
    name,
    is_configured: true,
    provider_type: 'Builtin',
    metadata: {
      config_keys: [],
      default_model: '',
      description: '',
      display_name: name,
      known_models: [],
      model_doc_link: '',
      name,
      tier: backend.tier ?? 'public',
      runs_locally: backend.runs_locally ?? false,
    },
  } as ProviderDetails;
}

const all = [
  provider('llamacpp', { tier: 'private', runs_locally: true }),
  provider('versa_azure', { tier: 'private' }),
  provider('azure_openai'),
  provider('anthropic'),
];

describe('ProviderGrid — the privacy taxonomy, on screen', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // The served copy is held in module state (one install, one disclosure), so
    // it outlives `cleanup()` and has to be dropped between tests.
    __resetDisclosureStoreForTests();
    mocks.getPrivacyDisclosure.mockResolvedValue({
      data: {
        title_template: '{provider} is not hosted by your institution.',
        long: 'SERVED-LONG-MARKER',
        short: 'SERVED-SHORT-MARKER — this model can read files on this computer.',
        acknowledged: true,
      },
    });
  });

  it('the provider grid headers name the two taxonomies with the same words', () => {
    render(<ProviderGrid providers={all} isOnboarding={false} />);

    expect(screen.getByText(/Private · Local/)).toBeInTheDocument();
    expect(screen.getByText(/Private · Institutional/)).toBeInTheDocument();
    expect(screen.getByText(/Public · Commercial/)).toBeInTheDocument();

    // The old words are gone from the SCREEN, which is the half a data-only
    // relabel cannot satisfy.
    expect(screen.queryByText('Local Models')).toBeNull();
    expect(screen.queryByText('Institutional Models')).toBeNull();
    expect(screen.queryByText('Commercial Models')).toBeNull();
  });

  it('says why an institutional endpoint is private, and why a cloud account is not', () => {
    render(<ProviderGrid providers={all} isOnboarding={false} />);

    // §14.5, verbatim: the reason is the recognised endpoint, not the vendor.
    expect(screen.getByText(/recognises this institutional gateway endpoint/i)).toBeInTheDocument();

    // §14.5's note: NOT "a direct cloud account, even if your institution pays
    // for it" — `azure.rs` defaults AZURE_OPENAI_ENDPOINT to the UCSF gateway
    // itself, so that wording would claim something the configuration
    // contradicts.
    expect(screen.getByText(/can't verify where/i)).toHaveTextContent(/endpoint points/i);
  });

  /**
   * Task 30A (issue #56, DR-17 requirement 3). The Commercial section is one of
   * the surfaces that carries the disclosure permanently — it reads no
   * acknowledgement and never goes quiet, which is what makes "shown once,
   * forcefully" a defensible design rather than a one-off popup.
   */
  it('the Commercial section says what a model there can reach, in the served words', async () => {
    render(<ProviderGrid providers={all} isOnboarding={false} />);
    const note = await screen.findByTestId('non-private-model-note');
    expect(note).toHaveTextContent(/SERVED-SHORT-MARKER/);
    expect(note).toHaveTextContent(/can read files on this computer/i);
  });

  it('renders nothing there rather than inventing prose when the copy cannot be fetched', async () => {
    mocks.getPrivacyDisclosure.mockRejectedValue(new Error('offline'));
    render(<ProviderGrid providers={all} isOnboarding={false} />);
    expect(await screen.findByText(/Public · Commercial/)).toBeInTheDocument();
    expect(screen.queryByTestId('non-private-model-note')).toBeNull();
  });
});
