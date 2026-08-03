import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ModelsBottomBar from './ModelsBottomBar';

const mocks = vi.hoisted(() => ({
  read: vi.fn(async () => ''),
  getProviders: vi.fn(async () => [] as unknown[]),
  // Task 30A: mutable, because the disclosure line depends on the tier of the
  // PROVIDER bound to the chat, not on the chat's own classification — a fresh
  // chat on Versa is classified `public` and its model is emphatically not.
  currentProvider: 'versa_azure',
  getPrivacyDisclosure: vi.fn(),
  ackPrivacyDisclosure: vi.fn(),
}));

vi.mock('../../../../api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getPrivacyDisclosure: mocks.getPrivacyDisclosure,
  ackPrivacyDisclosure: mocks.ackPrivacyDisclosure,
}));
vi.mock('../../../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

// ⚠ `usePrivacyTiersEnabled` as well as `useConfig`. `PrivacyBadge` reads the
// master switch itself (issue #56, DR-15) rather than making its nine call
// sites remember a prop, so any test that mocks this module AND renders a badge
// owes both. Omitting it fails loudly — "usePrivacyTiersEnabled is not a
// function" at the badge's own line — which is the failure mode to want; the
// alternative was a prop that ships unpassed, and it did.
vi.mock('../../../ConfigContext', () => ({
  useConfig: () => ({ read: mocks.read, getProviders: mocks.getProviders }),
  usePrivacyTiersEnabled: () => true,
}));

vi.mock('../../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    currentModel: 'claude-opus-4',
    currentProvider: mocks.currentProvider,
    getCurrentModelAndProviderForDisplay: async () => ({
      model: 'claude-opus-4',
      provider: 'Versa',
    }),
    getCurrentModelDisplayName: async () => 'claude-opus-4',
    getCurrentProviderDisplayName: async () => 'Versa',
  }),
}));

// `BaseChat` is the whole chat surface; importing it for one dead context would
// pull half the app into this suite.
vi.mock('../../../BaseChat', () => ({ useCurrentModelInfo: () => null }));

vi.mock('../subcomponents/SwitchModelModal', () => ({ SwitchModelModal: () => null }));
vi.mock('../subcomponents/LeadWorkerSettings', () => ({ LeadWorkerSettings: () => null }));

// `RefObject<HTMLDivElement>` is non-nullable in this React version, and the
// bar only forwards it to a wrapper `div`.
const dropdownRef = { current: null } as unknown as React.RefObject<HTMLDivElement>;

function renderBar(privacyTier?: 'public' | 'private') {
  return render(
    <ModelsBottomBar
      sessionId="s1"
      privacyTier={privacyTier}
      dropdownRef={dropdownRef}
      setView={vi.fn()}
      alerts={[]}
    />
  );
}

const providerEntry = (name: string, tier: 'private' | 'public') => ({
  name,
  is_configured: true,
  provider_type: 'Builtin',
  metadata: { name, display_name: name, tier, runs_locally: tier === 'private' },
});

describe('ModelsBottomBar — the chip carries a dot, never a pill', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentProvider = 'versa_azure';
    mocks.getProviders.mockResolvedValue([
      providerEntry('versa_azure', 'private'),
      providerEntry('openai', 'public'),
    ]);
    mocks.getPrivacyDisclosure.mockResolvedValue({
      data: {
        title_template: '{provider} is not hosted by your institution.',
        long: 'SERVED-LONG-MARKER',
        short: 'SERVED-SHORT-MARKER — this model can read files on this computer.',
        acknowledged: true,
      },
    });
  });

  it('marks a private chat with the dense badge and no added text', () => {
    renderBar('private');

    const badge = screen.getByTestId('privacy-badge');
    expect(badge).toHaveAttribute('data-privacy', 'private');
    // The trigger is `max-w-[120px]` with a 24-character truncation: a "Private"
    // pill cannot fit, so the dense form is the only one this surface may use.
    expect(badge).not.toHaveTextContent(/Private/);
  });

  it('leaves a public chat unmarked', () => {
    renderBar('public');
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });

  it('says the word in the dropdown header, where there is room for it', async () => {
    renderBar('private');
    fireEvent.pointerDown(screen.getByLabelText(/Current model/), {
      button: 0,
      ctrlKey: false,
    });

    expect(await screen.findByText(/Private chat/i)).toBeInTheDocument();
  });

  /**
   * Task 30A (issue #56, DR-17 requirement 3). The chip is where a user looks
   * to see which model they are talking to, so it is where the one-line
   * disclosure belongs.
   *
   * ⚠ It hangs off the bound PROVIDER's tier, never off `privacyTier`. That
   * prop is the chat's ratcheted CLASSIFICATION: a fresh chat on Versa is
   * classified `public` and its model is emphatically not a public model, so a
   * line keyed on it would tell the user something false about Versa.
   */
  it('a public model gets the one-line disclosure in the dropdown', async () => {
    mocks.currentProvider = 'openai';
    renderBar('public');
    fireEvent.pointerDown(screen.getByLabelText(/Current model/), { button: 0, ctrlKey: false });
    expect(await screen.findByTestId('non-private-model-chip-note')).toHaveTextContent(
      /SERVED-SHORT-MARKER/
    );
  });

  it('an institutional model does NOT, even while the chat itself is still public', async () => {
    mocks.currentProvider = 'versa_azure';
    renderBar('public');
    fireEvent.pointerDown(screen.getByLabelText(/Current model/), { button: 0, ctrlKey: false });
    await screen.findByText(/Current model/);
    expect(screen.queryByTestId('non-private-model-chip-note')).toBeNull();
  });
});
