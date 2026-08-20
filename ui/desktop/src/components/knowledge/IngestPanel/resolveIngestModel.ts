import type { ModelRef } from '../../../api/types.gen';

/**
 * The providers a knowledge macro cannot use, and the ONE place that fact is
 * written down.
 *
 * `claude_code` and `codex` are the coding-agent providers: they drive another
 * vendor's installed CLI as a subprocess. Their `complete_with_model` accepts a
 * `tools` argument and does not forward it (`_tools` in
 * `crates/biorouter/src/providers/{claude_code,codex}.rs`) — the child CLI's
 * tool surface arrives over the MCP bridge, which only the agent's own chat-turn
 * loop establishes. A knowledge macro (ingest / query / lint) calls the provider
 * directly, so on that path the model is handed no tools at all.
 *
 * What that looks like, measured: the model produces a complete, correct plan
 * with every call written out as prose (`<tool_call>{"name": "kb_write", …}`),
 * invents its own `<tool_response>OK</tool_response>` replies to continue
 * against, and nothing reaches disk. The daemon now recognises the shape after
 * the fact (`narrated_its_tool_calls` in
 * `crates/biorouter-mcp/src/knowledge/macros/ingest.rs`) and fails with an
 * explanation, but the user has already waited out a full model run for nothing.
 * Not offering the choice is the better fix.
 *
 * ⚠ **This is an ingest-surface exclusion only.** Both are good CHAT providers,
 * verified end to end including tool calls over the bridge, and
 * `providerOrdering.ts` deliberately keeps no hide-list — a filter there would
 * mean the settings grid silently contradicting the daemon it renders. Keep the
 * names here.
 *
 * ⚠ **Named, not detected, because nothing on the wire carries the fact.**
 * `ProviderMetadata` has no "forwards tools" field, so there is no data-driven
 * key to filter on; a renderer cannot tell these two from any other provider.
 * The moment either one forwards `tools` on the direct-completion path — or a
 * capability flag appears in `ProviderMetadata` — delete its entry here (or
 * replace the whole list with the flag) rather than leaving a stale name
 * suppressing a provider that works.
 */
export const PROVIDERS_WITHOUT_INGEST_TOOL_CALLS: readonly string[] = ['claude_code', 'codex'];

/** Whether a provider can carry the tool calls a knowledge macro is made of. */
export function providerCanRunIngest(providerName: string): boolean {
  return !PROVIDERS_WITHOUT_INGEST_TOOL_CALLS.includes(providerName);
}

/**
 * Decide which model the ingest panel should preselect.
 *
 * Precedence:
 *  1. the knowledge base's own `default_model`, when it has one — an explicit
 *     per-base override the user (or a scheduled job) already committed to;
 *  2. otherwise the provider/model the app is configured with — the same pair
 *     the chat composer shows, read from `useModelAndProvider`.
 *
 * A candidate from a provider in `PROVIDERS_WITHOUT_INGEST_TOOL_CALLS` is
 * skipped at both steps. It is not a matter of taste: that model cannot write a
 * page, so preselecting it only buys the user a full model run that ends with
 * an empty knowledge base. A stored `default_model` naming one is skipped too —
 * it could only have been written before this exclusion existed, and honouring
 * it would dispatch the run this function exists to prevent.
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
  if (
    kbDefaultModel?.provider &&
    kbDefaultModel.model &&
    providerCanRunIngest(kbDefaultModel.provider)
  ) {
    return { provider: kbDefaultModel.provider, model: kbDefaultModel.model };
  }
  if (configuredProvider && configuredModel && providerCanRunIngest(configuredProvider)) {
    return { provider: configuredProvider, model: configuredModel };
  }
  return null;
}

/**
 * Did resolution come back empty *because* every candidate was a provider
 * ingest cannot use?
 *
 * The distinction is the whole reason this exists. "No model configured" is a
 * verdict on the user's setup and it is simply false here — they have a model,
 * it is bound to the chat composer, and it works there. Sending them to Settings
 * to configure something they already configured is the kind of wrong answer
 * that costs an afternoon. The picker uses this to say what is actually true
 * instead.
 */
export function ingestModelBlockedByProvider(
  kbDefaultModel: ModelRef | null | undefined,
  configuredProvider: string | null | undefined,
  configuredModel: string | null | undefined
): boolean {
  if (resolveIngestModel(kbDefaultModel, configuredProvider, configuredModel)) return false;
  const candidateProviders = [
    kbDefaultModel?.provider && kbDefaultModel.model ? kbDefaultModel.provider : null,
    configuredProvider && configuredModel ? configuredProvider : null,
  ].filter((name): name is string => Boolean(name));
  // Resolution already failed, so any candidate still standing here is one this
  // module refused — an empty list means there was nothing to refuse.
  return candidateProviders.length > 0;
}
