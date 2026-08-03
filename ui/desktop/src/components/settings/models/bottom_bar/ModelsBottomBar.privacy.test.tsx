import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ModelsBottomBar from './ModelsBottomBar';

const mocks = vi.hoisted(() => ({
  read: vi.fn(async () => ''),
  getProviders: vi.fn(async () => []),
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
    currentProvider: 'versa_azure',
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

describe('ModelsBottomBar — the chip carries a dot, never a pill', () => {
  beforeEach(() => vi.clearAllMocks());

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
});
