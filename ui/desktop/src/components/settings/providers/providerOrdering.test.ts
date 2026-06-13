import { describe, expect, it } from 'vitest';
import type { ProviderDetails } from '../../../api';
import { getOrderedProviderGroups } from './providerOrdering';

function provider(name: string, displayName = name): ProviderDetails {
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
    },
  } as ProviderDetails;
}

describe('getOrderedProviderGroups', () => {
  it('groups providers in settings-page order (local first)', () => {
    const groups = getOrderedProviderGroups([
      provider('openai'),
      provider('ollama'),
      provider('versa_bedrock'),
      provider('llamacpp'),
      provider('anthropic'),
      provider('versa_azure'),
    ]);

    expect(groups.map((group) => group.key)).toEqual(['local', 'institutional', 'commercial']);
    expect(groups[0]?.providers.map((item) => item.name)).toEqual(['llamacpp', 'ollama']);
    expect(groups[1]?.providers.map((item) => item.name)).toEqual(['versa_azure', 'versa_bedrock']);
    expect(groups[2]?.providers.map((item) => item.name)).toEqual(['anthropic', 'openai']);
  });

  it('ranks Llama Server before Ollama within local models', () => {
    const groups = getOrderedProviderGroups([provider('ollama'), provider('llamacpp')]);
    expect(groups[0]?.key).toBe('local');
    expect(groups[0]?.providers.map((item) => item.name)).toEqual(['llamacpp', 'ollama']);
  });

  it('filters hidden providers from all groups', () => {
    const groups = getOrderedProviderGroups([
      provider('codex'),
      provider('cursor-agent'),
      provider('openai'),
    ]);

    expect(groups[2]?.providers.map((item) => item.name)).toEqual(['openai']);
  });
});
