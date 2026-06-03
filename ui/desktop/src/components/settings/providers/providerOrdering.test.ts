import { describe, expect, it } from 'vitest';
import type { ProviderDetails } from '../../../api';
import { getOrderedProviderGroups } from './providerOrdering';

function provider(
  name: string,
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
    },
  } as ProviderDetails;
}

describe('getOrderedProviderGroups', () => {
  it('groups providers in settings-page order', () => {
    const groups = getOrderedProviderGroups([
      provider('openai'),
      provider('ollama'),
      provider('versa_bedrock'),
      provider('anthropic'),
      provider('versa_azure'),
    ]);

    expect(groups.map((group) => group.key)).toEqual([
      'institutional',
      'local',
      'commercial',
    ]);
    expect(groups[0]?.providers.map((item) => item.name)).toEqual([
      'versa_azure',
      'versa_bedrock',
    ]);
    expect(groups[1]?.providers.map((item) => item.name)).toEqual(['ollama']);
    expect(groups[2]?.providers.map((item) => item.name)).toEqual([
      'anthropic',
      'openai',
    ]);
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
