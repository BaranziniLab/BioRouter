import { ProviderDetails } from '../../../api';

export default interface Model {
  id?: number; // Make `id` optional to allow user-defined models
  name: string;
  provider: string;
  lastUsed?: string;
  alias?: string; // optional model display name
  subtext?: string; // goes below model name if not the provider
  context_limit?: number; // optional context limit override
  request_params?: Record<string, unknown>; // provider-specific request parameters
}

export function createModelStruct(
  modelName: string,
  provider: string,
  id?: number, // Make `id` optional to allow user-defined models
  lastUsed?: string,
  alias?: string, // optional model display name
  subtext?: string
): Model {
  // use the metadata to create a Model
  return {
    name: modelName,
    provider: provider,
    alias: alias,
    id: id,
    lastUsed: lastUsed,
    subtext: subtext,
  };
}

export async function getProviderMetadata(
  providerName: string,
  getProvidersFunc: (b: boolean) => Promise<ProviderDetails[]>
) {
  const providers = await getProvidersFunc(false);
  const matches = providers.find((providerMatch) => providerMatch.name === providerName);
  if (!matches) {
    throw Error(`No match for provider: ${providerName}`);
  }
  return matches.metadata;
}

export interface ProviderModelsResult {
  provider: ProviderDetails;
  models: string[] | null;
  error: string | null;
}

/**
 * Fetches models for all active providers in parallel.
 * When a provider has a curated known_models list, that is used exclusively
 * (avoids showing deprecated/unsupported models from the live API).
 * For providers without known_models (e.g. Ollama), falls back to live API.
 */
export async function fetchModelsForProviders(
  activeProviders: ProviderDetails[],
  getProviderModelsFunc: (providerName: string) => Promise<string[]>
): Promise<ProviderModelsResult[]> {
  const modelPromises = activeProviders.map(async (p) => {
    const providerName = p.name;
    const knownModels = p.metadata.known_models?.map((m) => m.name) || [];

    // When a curated known_models list exists, use it exclusively so only
    // tested/supported models are shown (live API includes old deprecated models).
    if (knownModels.length > 0) {
      return { provider: p, models: knownModels, error: null };
    }

    // No curated list — use the live API (e.g. Ollama, custom providers).
    try {
      const models = await getProviderModelsFunc(providerName);
      return { provider: p, models: models || [], error: null };
    } catch (e: unknown) {
      const errorMessage = `Failed to fetch models for ${providerName}${e instanceof Error ? `: ${e.message}` : ''}`;
      return {
        provider: p,
        models: null,
        error: errorMessage,
      };
    }
  });

  return await Promise.all(modelPromises);
}
