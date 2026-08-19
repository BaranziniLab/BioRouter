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
    // The real shape this covers: a custom_providers/*.json named `ollama`
    // shadows the built-in registry entry (registration is a plain insert by
    // config.name, after the built-ins), and the declarative path defaults the
    // tier to public rather than inheriting the engine's. The renderer must
    // follow the backend rather than recognising the name, or a provider that
    // is not the real ollama keeps a "Private · Local" badge it never earned.
    //
    // What this does NOT yet cover, deliberately: GET /config/providers serves
    // the *type-level* metadata, so a genuine built-in ollama re-pointed off
    // this machine by OLLAMA_HOST still ships tier: 'private' here even though
    // its instance Provider::tier() resolves public. Enforcement reads the
    // instance method and is unaffected; this grid does not. Feeding the
    // instance tier to the UI is follow-up work, and it must land before any
    // user-facing privacy badge is hung on metadata.tier — that badge would be
    // wrong in exactly the demotion case the tier exists to catch.
    const groups = getOrderedProviderGroups([provider('ollama', { tier: 'public' })]);
    expect(groups[0]?.providers).toEqual([]);
    expect(groups[2]?.providers.map((item) => item.name)).toEqual(['ollama']);
  });

  it('hides nothing: every provider the daemon serves reaches a group', () => {
    // This replaced a hide-list test for `codex` / `cursor-agent` / `claude-code`.
    // Two of those names are back — the daemon serves `claude_code` and `codex`
    // deliberately now — and the assertion is written this way precisely so that
    // it keeps holding: what must not come back is a renderer-side list of
    // names, so a provider absent from this grid is absent because the daemon
    // did not send it, never because this module recognised it.
    const groups = getOrderedProviderGroups([
      provider('openai'),
      provider('ollama', PRIVATE_LOCAL),
      provider('versa_azure', PRIVATE_REMOTE),
    ]);

    expect(groups.flatMap((group) => group.providers.map((item) => item.name)).sort()).toEqual([
      'ollama',
      'openai',
      'versa_azure',
    ]);
  });

  it('sorts the CLI-wrapper providers into commercial, behind the direct ones', () => {
    // `claude_code` and `codex` drive a CLI the user already has installed, on
    // their own subscription — a distinct kind of thing from a direct cloud
    // account, so they rank after the direct vendors instead of falling to the
    // 999 default, where localeCompare alone would have put `claude_code` ahead
    // of `openai`. The daemon declares both public, which is the only reason
    // they are in this group at all.
    const groups = getOrderedProviderGroups([
      provider('codex', {}, 'Codex'),
      provider('openai'),
      provider('claude_code', {}, 'Claude Code'),
      provider('anthropic'),
    ]);

    expect(groups[2]?.key).toBe('commercial');
    expect(groups[2]?.providers.map((item) => item.name)).toEqual([
      'anthropic',
      'openai',
      'claude_code',
      'codex',
    ]);
  });
});
