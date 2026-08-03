import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProviderDetails, ProviderTier } from '../../../../api';
import { SwitchModelModal } from './SwitchModelModal';

const mocks = vi.hoisted(() => ({
  getProviders: vi.fn(),
  getProviderModels: vi.fn(),
  read: vi.fn(),
  changeModel: vi.fn(),
}));

vi.mock('../../../ConfigContext', () => ({
  useConfig: () => ({
    getProviders: mocks.getProviders,
    getProviderModels: mocks.getProviderModels,
    read: mocks.read,
  }),
}));

vi.mock('../../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    changeModel: mocks.changeModel,
    // `null`, so the modal's own auto-select effect fills the field from the
    // fetched list — which is also how we know the options finished loading.
    currentModel: null,
    currentProvider: 'anthropic',
  }),
}));

vi.mock('../predefinedModelsUtils', () => ({
  getPredefinedModelsFromEnv: () => [],
  shouldShowPredefinedModels: () => false,
}));

// ⚠ The sibling suite `SwitchModelModal.test.tsx` replaces `ui/Select` with a
// stub that renders only the *value*. Nothing in it can see an option row, let
// alone a disabled one, so this file deliberately runs the REAL react-select:
// `role="option"` and `aria-disabled` are react-select's own output, and they
// are exactly what the pre-flight state has to produce.

function provider(
  name: string,
  tier: ProviderTier | undefined,
  displayName = name
): ProviderDetails {
  return {
    name,
    is_configured: true,
    provider_type: 'Builtin',
    metadata: {
      config_keys: [],
      default_model: '',
      description: '',
      display_name: displayName,
      known_models: [],
      model_doc_link: '',
      name,
      tier,
      runs_locally: false,
    },
  } as ProviderDetails;
}

/**
 * Open the model menu. The provider combobox is first, the model one second.
 *
 * ⚠ Wait for the auto-selected model to appear first. react-select is
 * `isDisabled` while `loadingModels`, and an ArrowDown fired before the fetch
 * settles opens a menu with no options — which reads exactly like a missing
 * feature.
 */
async function openModelMenu() {
  await screen.findByText('Claude Opus 4.8');
  const combos = screen.getAllByRole('combobox');
  fireEvent.keyDown(combos[combos.length - 1], { key: 'ArrowDown', code: 'ArrowDown' });
}

describe('SwitchModelModal — pre-flight, not post-refusal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getProviders.mockResolvedValue([
      provider('anthropic', 'public', 'Anthropic'),
      provider('versa_azure', 'private', 'Versa'),
    ]);
    mocks.getProviderModels.mockResolvedValue(['Claude Opus 4.8']);
    mocks.read.mockResolvedValue('');
    mocks.changeModel.mockResolvedValue(true);
  });

  it('a public model is disabled with its reason in a private chat', async () => {
    render(
      <SwitchModelModal sessionId="s1" privacyTier="private" onClose={vi.fn()} setView={vi.fn()} />
    );

    await openModelMenu();
    const row = await screen.findByRole('option', { name: /Claude Opus/ });
    expect(row).toHaveAttribute('aria-disabled', 'true');
    expect(row).toHaveTextContent(/private chat/i);
  });

  // Without this the assertion above passes for a modal that disables EVERY
  // row, which would be a worse bug than the one it is meant to catch.
  it('leaves the same row selectable in a public chat', async () => {
    render(
      <SwitchModelModal sessionId="s1" privacyTier="public" onClose={vi.fn()} setView={vi.fn()} />
    );

    await openModelMenu();
    const row = await screen.findByRole('option', { name: /Claude Opus/ });
    expect(row).toHaveAttribute('aria-disabled', 'false');
    expect(row).not.toHaveTextContent(/private chat/i);
  });

  // A tier the caller could not resolve is not an assertion that the chat is
  // public: the settings grid opens this modal with no session at all.
  it('judges nothing when the chat tier is unknown', async () => {
    render(<SwitchModelModal sessionId={null} onClose={vi.fn()} setView={vi.fn()} />);

    await openModelMenu();
    const row = await screen.findByRole('option', { name: /Claude Opus/ });
    expect(row).toHaveAttribute('aria-disabled', 'false');
  });

  // ⚠ An ABSENT tier is not an unknown one — it is Public, and it has to be
  // read that way here or the pre-flight silently stops covering it.
  //
  // `ProviderMetadata::tier` is `#[serde(default)]` over a `ProviderTier` whose
  // `Default` is deliberately `Public` ("fail-safe, not fail-open: a provider
  // module that forgets `tier()` gets less reach, never more"), which is why
  // the generated client types the field as optional. A daemon predating the
  // field, or any provider whose metadata omits it, therefore arrives here with
  // `tier === undefined` while the daemon resolves it to Public and
  // `available_private_providers` — which filters on `is_private()` — declines
  // to offer it. A `=== 'public'` test would leave that row selectable, then
  // hand the user a Gate A 409: the false-negative this whole task exists to
  // replace.
  it('treats a provider with no declared tier as public, exactly as the daemon does', async () => {
    mocks.getProviders.mockResolvedValue([
      provider('mystery', undefined, 'Mystery'),
      provider('versa_azure', 'private', 'Versa'),
    ]);

    render(
      <SwitchModelModal
        sessionId="s1"
        privacyTier="private"
        initialProvider="mystery"
        onClose={vi.fn()}
        setView={vi.fn()}
      />
    );

    await openModelMenu();
    const row = await screen.findByRole('option', { name: /Claude Opus/ });
    expect(row).toHaveAttribute('aria-disabled', 'true');
    expect(row).toHaveTextContent(/private chat/i);
  });

  // The other half of the same predicate: a declared-private provider stays
  // selectable, so the change above cannot have been "block everything".
  it('leaves a declared-private provider selectable in a private chat', async () => {
    render(
      <SwitchModelModal
        sessionId="s1"
        privacyTier="private"
        initialProvider="versa_azure"
        onClose={vi.fn()}
        setView={vi.fn()}
      />
    );

    await openModelMenu();
    const row = await screen.findByRole('option', { name: /Claude Opus/ });
    expect(row).toHaveAttribute('aria-disabled', 'false');
  });
});
