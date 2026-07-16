import { render, waitFor } from '@testing-library/react';
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
