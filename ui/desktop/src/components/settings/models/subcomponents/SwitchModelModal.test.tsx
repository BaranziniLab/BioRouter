import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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
    currentModel: 'claude-old',
    currentProvider: 'anthropic',
  }),
}));

vi.mock('../predefinedModelsUtils', () => ({
  getPredefinedModelsFromEnv: () => [],
  shouldShowPredefinedModels: () => false,
}));

vi.mock('../../../ui/Select', () => ({
  Select: ({ value, placeholder }: { value?: { value?: string } | null; placeholder?: string }) => (
    <div data-testid={placeholder?.startsWith('Provider') ? 'provider-select' : 'model-select'}>
      {value?.value || placeholder}
    </div>
  ),
}));

describe('SwitchModelModal onboarding initialization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getProviders.mockResolvedValue([
      {
        name: 'openai',
        is_configured: true,
        provider_type: 'Commercial',
        metadata: {
          name: 'openai',
          display_name: 'OpenAI',
          default_model: 'gpt-4o',
          known_models: [{ name: 'gpt-4o' }],
          allows_unlisted_models: false,
          config_keys: [],
        },
      },
    ]);
    mocks.getProviderModels.mockResolvedValue(['gpt-4o']);
    mocks.read.mockResolvedValue('');
    mocks.changeModel.mockResolvedValue(true);
  });

  it('refreshes the provider catalog when onboarding supplies a detected provider', async () => {
    render(
      <SwitchModelModal
        sessionId={null}
        onClose={vi.fn()}
        setView={vi.fn()}
        initialProvider="openai"
        initialModel="gpt-4o"
      />
    );

    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(mocks.getProviders).toHaveBeenNthCalledWith(1, true);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="provider-select"]')).toHaveTextContent('openai');
      expect(document.querySelector('[data-testid="model-select"]')).toHaveTextContent('gpt-4o');
    });
  });
});

/**
 * What the dialog does while a bind is in flight, and what it says when one
 * fails.
 *
 * Reported as *"the Select Model button actually froze without telling me
 * why"* against Versa API Azure. The button had not frozen: `changeModel` was
 * awaiting the daemon, the button carried no pending state to show it, and on
 * failure `handleSubmit` returned leaving the dialog byte-for-byte as it was.
 * Every report the user got lived in a toast in the opposite corner of the
 * screen — including `TypeError: Failed to fetch`, which is what the underlying
 * fault (a runaway catalogue poll exhausting the renderer's socket pool)
 * produced. A dialog that cannot say "working" or "that failed" is
 * indistinguishable from a dead one.
 */
describe('SwitchModelModal switch feedback', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getProviders.mockResolvedValue([
      {
        name: 'versa_azure',
        is_configured: true,
        provider_type: 'Institutional',
        metadata: {
          name: 'versa_azure',
          display_name: 'Versa API Azure',
          default_model: 'gpt-5.5-2026-04-24',
          known_models: [{ name: 'gpt-5.5-2026-04-24' }],
          allows_unlisted_models: false,
          config_keys: [],
        },
      },
    ]);
    mocks.getProviderModels.mockResolvedValue(['gpt-5.5-2026-04-24']);
    mocks.read.mockResolvedValue('');
  });

  const renderModal = (onClose = vi.fn()) =>
    render(
      <SwitchModelModal
        sessionId="s-1"
        onClose={onClose}
        setView={vi.fn()}
        initialProvider="versa_azure"
        initialModel="gpt-5.5-2026-04-24"
      />
    );

  const clickSelect = async () => {
    const button = await waitFor(() => {
      const found = screen.getAllByRole('button').find((el) => el.textContent === 'Select model');
      if (!found) throw new Error('Select model button not rendered');
      return found;
    });
    fireEvent.click(button);
    return button;
  };

  it('shows the switch is running, and refuses a second one while it is', async () => {
    let release: (ok: boolean) => void = () => {};
    mocks.changeModel.mockImplementation(
      () =>
        new Promise<boolean>((resolve) => {
          release = resolve;
        })
    );

    renderModal();
    const button = await clickSelect();

    // The label is the whole point: a bind crosses the network twice and the
    // user must be able to tell "working" from "ignored me".
    await waitFor(() => expect(button).toHaveTextContent('Switching'));
    expect(button).toBeDisabled();

    // A second click must not start a second bind — two racing binds write two
    // providers through `/config/set_provider` and the loser wins.
    fireEvent.click(button);
    expect(mocks.changeModel).toHaveBeenCalledTimes(1);

    await act(async () => {
      release(true);
    });
  });

  it('says so in the dialog when the switch did not happen', async () => {
    mocks.changeModel.mockResolvedValue(false);
    const onClose = vi.fn();

    renderModal(onClose);
    await clickSelect();

    await waitFor(() =>
      expect(screen.getByTestId('switch-model-submit-error')).toHaveTextContent(
        'The model was not switched'
      )
    );
    // A refused switch leaves the dialog open — closing it would hide the one
    // control that can retry.
    expect(onClose).not.toHaveBeenCalled();
    // And the button must come back, or the dialog is a dead end.
    await waitFor(() =>
      expect(
        screen.getAllByRole('button').find((el) => el.textContent === 'Select model')
      ).toBeEnabled()
    );
  });

  /**
   * ⚠ `handleSubmit` is wired straight to `onClick`, so nothing holds the
   * promise it returns. Before this, a throw anywhere inside it became an
   * unhandled rejection and the dialog did not move — the exact silent freeze
   * that was reported.
   */
  it('reports a thrown failure instead of leaving an unhandled rejection', async () => {
    mocks.changeModel.mockRejectedValue(new Error('Failed to fetch'));

    const unhandled: unknown[] = [];
    const onUnhandled = (event: PromiseRejectionEvent) => {
      unhandled.push(event.reason);
      event.preventDefault();
    };
    window.addEventListener('unhandledrejection', onUnhandled);

    try {
      renderModal();
      await clickSelect();

      await waitFor(() =>
        expect(screen.getByTestId('switch-model-submit-error')).toHaveTextContent('Failed to fetch')
      );
      await act(async () => {
        await Promise.resolve();
      });
    } finally {
      window.removeEventListener('unhandledrejection', onUnhandled);
    }

    expect(unhandled).toEqual([]);
  });
});
