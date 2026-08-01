import { describe, expect, it } from 'vitest';
import type { ProviderDetails, ProviderTier } from '../../../api';
import { getOrderedProviderGroups } from './providerOrdering';

/**
 * `tier` and `runs_locally` are what the daemon sends for this provider — the
 * grouping is derived from them, so the fixtures state them rather than letting
 * the renderer recognise a name.
 */
function provider(
  name: string,
  backend: { tier?: ProviderTier; runs_locally?: boolean } = {},
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
      tier: backend.tier ?? 'public',
      runs_locally: backend.runs_locally ?? false,
    },
  } as ProviderDetails;
}

const PRIVATE_LOCAL = { tier: 'private', runs_locally: true } as const;
const PRIVATE_REMOTE = { tier: 'private', runs_locally: false } as const;

describe('getOrderedProviderGroups', () => {
  it('groups providers in settings-page order (local first)', () => {
    const groups = getOrderedProviderGroups([
      provider('openai'),
      provider('ollama', PRIVATE_LOCAL),
      provider('versa_bedrock', PRIVATE_REMOTE),
      provider('llamacpp', PRIVATE_LOCAL),
      provider('anthropic'),
      provider('versa_azure', PRIVATE_REMOTE),
    ]);

    expect(groups.map((group) => group.key)).toEqual(['local', 'institutional', 'commercial']);
    expect(groups[0]?.providers.map((item) => item.name)).toEqual(['llamacpp', 'ollama']);
    expect(groups[1]?.providers.map((item) => item.name)).toEqual(['versa_azure', 'versa_bedrock']);
    expect(groups[2]?.providers.map((item) => item.name)).toEqual(['anthropic', 'openai']);
  });

  it('ranks Llama Server before Ollama within local models', () => {
    const groups = getOrderedProviderGroups([
      provider('ollama', PRIVATE_LOCAL),
      provider('llamacpp', PRIVATE_LOCAL),
    ]);
    expect(groups[0]?.key).toBe('local');
    expect(groups[0]?.providers.map((item) => item.name)).toEqual(['llamacpp', 'ollama']);
  });

  it('demotes a private provider to commercial when the daemon says it is public', () => {
    // An ollama pointed off this machine resolves Public. The renderer must
    // follow the backend rather than recognising the name, or a demoted
    // provider keeps a "Local Models" badge it no longer earns.
    const groups = getOrderedProviderGroups([provider('ollama', { tier: 'public' })]);
    expect(groups[0]?.providers).toEqual([]);
    expect(groups[2]?.providers.map((item) => item.name)).toEqual(['ollama']);
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
