import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProviderDetails } from '../../../api';
import ProviderGrid from './ProviderGrid';
import { __resetDisclosureStoreForTests } from '../../privacy/disclosureCopy';
import { BROWSER_SURFACE_MARKER } from '../../../utils/surface';

/**
 * SD-1 on Settings > Providers.
 *
 * ⚠ This page is a *partial* block, and the test asserts the partiality rather
 * than only the note. Saving a provider's API key is not a capability write and
 * still works from a browser; the step after it — choosing which model becomes
 * the default, in `SwitchModelModal` — is the one that 409s. Saying so at the
 * top of the page means the user learns it before pasting a secret rather than
 * after.
 */

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

function provider(name: string): ProviderDetails {
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
      tier: 'public',
      runs_locally: false,
    },
  } as ProviderDetails;
}

describe('ProviderGrid on a browser-served surface', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    __resetDisclosureStoreForTests();
    mocks.getPrivacyDisclosure.mockResolvedValue({
      data: {
        title_template: '{provider} is not hosted by your institution.',
        long: 'SERVED-LONG-MARKER',
        short: 'SERVED-SHORT-MARKER',
        acknowledged: true,
      },
    });
  });

  afterEach(() => {
    delete document.documentElement.dataset.biorouterSurface;
  });

  /**
   * ⚠ Fails against today's code: nothing on this page mentions the host, so a
   * browser user configures a provider, is handed the model picker, and only
   * then meets a refusal written for an AI agent.
   */
  it('says once, at the top, that the host owns the choice', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    render(<ProviderGrid providers={[provider('anthropic')]} isOnboarding={false} />);

    const note = await screen.findByTestId('host-managed-model-note');
    expect(note.textContent).toMatch(/biorouter configure/);
    // Still a provider page: the cards are not taken away, because storing a
    // key is not what gets refused.
    expect(screen.getByText('anthropic')).toBeInTheDocument();
    expect(screen.getByTestId('add-custom-provider-card')).toBeInTheDocument();
  });

  /** The control: passes before and after. */
  it('adds nothing in the desktop application', async () => {
    render(<ProviderGrid providers={[provider('anthropic')]} isOnboarding={false} />);
    // Settle the disclosure fetch first, so this asserts the resolved page
    // rather than winning a race against a note that has not arrived yet.
    await screen.findByTestId('non-private-model-note');
    expect(screen.queryByTestId('host-managed-model-note')).toBeNull();
  });
});
