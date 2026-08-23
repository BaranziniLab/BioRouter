import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SwitchModelModal } from './SwitchModelModal';
import { BROWSER_SURFACE_MARKER } from '../../../../utils/surface';

/**
 * SD-1 at the choke point: `ModelsBottomBar`, `ModelSettingsButtons`,
 * `ProviderGrid` and `ProviderGuard` all open this one dialog, and its confirm
 * button reaches `changeModel` → `POST /config/set_provider`, which a
 * browser-served daemon refuses with a 409. Guarding only the four entry points
 * would leave the fifth one somebody adds next year unguarded.
 */

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
    currentModel: null,
    currentProvider: 'openai',
  }),
}));

vi.mock('../predefinedModelsUtils', () => ({
  getPredefinedModelsFromEnv: () => [],
  shouldShowPredefinedModels: () => false,
}));

// A stub that reports its own `isDisabled`, so the inert-input half of the
// requirement is observable. The real react-select renders no attribute a
// jsdom query can reach for a disabled *container*.
vi.mock('../../../ui/Select', () => ({
  Select: ({
    value,
    placeholder,
    isDisabled,
  }: {
    value?: { value?: string } | null;
    placeholder?: string;
    isDisabled?: boolean;
  }) => (
    <div
      data-testid={placeholder?.startsWith('Provider') ? 'provider-select' : 'model-select'}
      data-disabled={isDisabled ? 'true' : 'false'}
    >
      {value?.value || placeholder}
    </div>
  ),
}));

function renderModal() {
  return render(
    <SwitchModelModal
      sessionId="s1"
      onClose={vi.fn()}
      setView={vi.fn()}
      initialProvider="openai"
      initialModel="gpt-4o"
    />
  );
}

describe('SwitchModelModal on a browser-served surface', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getProviders.mockResolvedValue([
      {
        name: 'openai',
        is_configured: true,
        provider_type: 'Commercial',
        affiliation: null,
        metadata: {
          name: 'openai',
          display_name: 'OpenAI',
          default_model: 'gpt-4o',
          known_models: [{ name: 'gpt-4o' }],
          config_keys: [],
          tier: 'public',
        },
      },
    ]);
    mocks.getProviderModels.mockResolvedValue(['gpt-4o']);
    mocks.read.mockResolvedValue('');
    mocks.changeModel.mockResolvedValue(true);
  });

  afterEach(() => {
    delete document.documentElement.dataset.biorouterSurface;
  });

  /**
   * ⚠ **Fails against today's code**, where "Select model" is enabled the
   * moment the dialog opens (`isValid` starts `true`) and there is no note to
   * find. Today a browser user clicks it, `changeModel` fires, and the only
   * thing they see is the agent-facing 409 rendered as an error toast.
   */
  it('disables the confirm and explains why, before anything is clicked', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    renderModal();

    const note = await screen.findByTestId('host-managed-model-note');
    expect(note.textContent).toMatch(/biorouter configure/);
    expect(note.textContent).toMatch(/private conversation/i);

    expect(screen.getByRole('button', { name: 'Select model' })).toBeDisabled();
  });

  /**
   * The disabled attribute is a claim about the button; this is a claim about
   * the WRITE. `handleSubmit` carries its own guard for a keyboard submit or a
   * call site that renders its own confirm, and without asserting the call
   * count a rewrite could drop that guard and still pass the test above.
   *
   * ⚠ Fails against today's code: the click lands, `changeModel` is called
   * once, and the daemon answers 409.
   */
  it('never reaches changeModel even if the confirm is clicked', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    renderModal();

    await screen.findByTestId('host-managed-model-note');
    fireEvent.click(screen.getByRole('button', { name: 'Select model' }));

    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(mocks.changeModel).not.toHaveBeenCalled();
  });

  /**
   * The pickers themselves, not only the confirm. A dialog whose selects still
   * respond while its one action is dead reads as a broken form rather than as
   * a decision made elsewhere.
   *
   * ⚠ Fails against today's code, which passes `isDisabled` only for
   * `loadingModels` — so both stubs report `false` once the catalog settles.
   */
  it('makes the provider and model pickers inert', async () => {
    document.documentElement.dataset.biorouterSurface = BROWSER_SURFACE_MARKER;
    renderModal();

    await screen.findByTestId('host-managed-model-note');
    await waitFor(() => {
      expect(screen.getByTestId('provider-select')).toHaveAttribute('data-disabled', 'true');
      expect(screen.getByTestId('model-select')).toHaveAttribute('data-disabled', 'true');
    });
  });

  /**
   * ⚠ **The control.** Passes before and after; it exists to catch a helper
   * that answers `browser` too readily. A false positive here does not merely
   * add a note — it takes the model picker away from every desktop user, which
   * is a far worse outcome than the 409 this change replaces.
   */
  it('leaves the desktop dialog fully usable', async () => {
    renderModal();

    await waitFor(() =>
      expect(screen.getByTestId('provider-select')).toHaveAttribute('data-disabled', 'false')
    );
    expect(screen.queryByTestId('host-managed-model-note')).toBeNull();

    const confirm = screen.getByRole('button', { name: 'Select model' });
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    await waitFor(() => expect(mocks.changeModel).toHaveBeenCalledTimes(1));
  });
});
