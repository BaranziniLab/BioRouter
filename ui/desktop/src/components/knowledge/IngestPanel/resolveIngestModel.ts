import type { ModelRef } from '../../../api/types.gen';

/**
 * Decide which model the ingest panel should preselect.
 *
 * Precedence:
 *  1. the knowledge base's own `default_model`, when it has one — an explicit
 *     per-base override the user (or a scheduled job) already committed to;
 *  2. otherwise the provider/model the app is configured with — the same pair
 *     the chat composer shows, read from `useModelAndProvider`.
 *
 * There is deliberately **no** hardcoded vendor fallback. If neither source
 * resolves, this returns `null` and the caller must say so and keep digestion
 * disabled rather than dispatching an agentic loop at a provider the user never
 * configured (see issue #46).
 */
export function resolveIngestModel(
  kbDefaultModel: ModelRef | null | undefined,
  configuredProvider: string | null | undefined,
  configuredModel: string | null | undefined
): ModelRef | null {
  if (kbDefaultModel?.provider && kbDefaultModel.model) {
    return { provider: kbDefaultModel.provider, model: kbDefaultModel.model };
  }
  if (configuredProvider && configuredModel) {
    return { provider: configuredProvider, model: configuredModel };
  }
  return null;
}
