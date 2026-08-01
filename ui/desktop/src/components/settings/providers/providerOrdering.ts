import type { ProviderDetails } from '../../../api';

const HIDDEN_PROVIDERS = new Set(['claude-code', 'codex', 'cursor-agent']);

const PRIORITY_ORDER: Record<string, number> = {
  versa_azure: 0,
  versa_bedrock: 1,
  llamacpp: 0,
  ollama: 1,
  azure_openai: 0,
  aws_bedrock: 1,
  anthropic: 2,
  openai: 3,
  google: 4,
  zai: 5,
  xiaomi_mimo: 6,
};

export type ProviderGroupKey = 'institutional' | 'local' | 'commercial';

export interface OrderedProviderGroup {
  key: ProviderGroupKey;
  label: string;
  accentClassName: string;
  providers: ProviderDetails[];
}

function compareProviders(a: ProviderDetails, b: ProviderDetails): number {
  const pa = PRIORITY_ORDER[a.name] ?? 999;
  const pb = PRIORITY_ORDER[b.name] ?? 999;
  if (pa !== pb) {
    return pa - pb;
  }
  return a.name.localeCompare(b.name);
}

/**
 * Grouping is the backend's answer, never a list kept here. `runs_locally` is
 * the display-only fact that splits the private tier into the two sections this
 * grid has always had. A renderer-side copy of either field is a second source
 * of truth that drifts silently the moment a provider is added, renamed, or
 * re-pointed.
 *
 * ⚠ `metadata.tier` is the *type-level* claim — the tier computed from the
 * endpoint a provider ships with — NOT the tier of the instance actually bound
 * to a session. `GET /config/providers` serves `ProviderMetadata` verbatim, and
 * for a built-in that struct is static, so an `ollama` re-pointed off this
 * machine by `OLLAMA_HOST` still arrives here as `private` while its instance
 * `Provider::tier()` resolves `public`. The two can only ever disagree in that
 * direction, which is harmless for choosing a section heading and is why this
 * module may read it.
 *
 * It is **not** harmless for a privacy badge: do not hang one on this field. A
 * badge has to read the tier of the bound instance, which means plumbing
 * `Provider::tier()` out to the UI first — hung here it would read Private in
 * exactly the demotion case the tier exists to catch. See the `tier` field's
 * doc comment in `crates/biorouter/src/providers/base.rs`.
 */
function classifyProvider(provider: ProviderDetails): ProviderGroupKey {
  if (provider.metadata.tier !== 'private') {
    return 'commercial';
  }
  return provider.metadata.runs_locally ? 'local' : 'institutional';
}

export function getOrderedProviderGroups(providers: ProviderDetails[]): OrderedProviderGroup[] {
  const visible = providers.filter((provider) => !HIDDEN_PROVIDERS.has(provider.name));
  const grouped: Record<ProviderGroupKey, ProviderDetails[]> = {
    institutional: [],
    local: [],
    commercial: [],
  };

  for (const provider of visible) {
    grouped[classifyProvider(provider)].push(provider);
  }

  grouped.institutional.sort(compareProviders);
  grouped.local.sort(compareProviders);
  grouped.commercial.sort(compareProviders);

  return [
    {
      key: 'local',
      label: 'Local Models',
      accentClassName: 'bg-green-500',
      providers: grouped.local,
    },
    {
      key: 'institutional',
      label: 'Institutional Models',
      accentClassName: 'bg-indigo-500',
      providers: grouped.institutional,
    },
    {
      key: 'commercial',
      label: 'Commercial Models',
      accentClassName: 'bg-amber-500',
      providers: grouped.commercial,
    },
  ];
}
