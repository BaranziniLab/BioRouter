import type { ProviderDetails } from '../../../api';

const HIDDEN_PROVIDERS = new Set(['claude-code', 'codex', 'cursor-agent']);
const INSTITUTIONAL = new Set(['versa_azure', 'versa_bedrock']);
const LOCAL = new Set(['llamacpp', 'ollama']);

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

function classifyProvider(provider: ProviderDetails): ProviderGroupKey {
  if (INSTITUTIONAL.has(provider.name)) {
    return 'institutional';
  }
  if (LOCAL.has(provider.name)) {
    return 'local';
  }
  return 'commercial';
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
