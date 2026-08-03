import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ProviderDetails, ProviderTier } from '../../../api';
import ProviderGrid from './ProviderGrid';

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
    expect(screen.getByText(/recognises this specific UCSF gateway endpoint/i)).toBeInTheDocument();

    // §14.5's note: NOT "a direct cloud account, even if your institution pays
    // for it" — `azure.rs` defaults AZURE_OPENAI_ENDPOINT to the UCSF gateway
    // itself, so that wording would claim something the configuration
    // contradicts.
    expect(screen.getByText(/can't verify where/i)).toHaveTextContent(/endpoint points/i);
  });
});
