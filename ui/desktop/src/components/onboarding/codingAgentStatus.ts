import { codingAgentsStatus } from '../../api';

/**
 * `GET /coding_agents/status` — is each vendor CLI installed, and is the user
 * signed in on a *subscription*?
 *
 * ## Why this module exists at all
 *
 * It wraps the generated `codingAgentsStatus` rather than calling it at each use
 * site, so the two provider-keyed maps below sit next to the shape they describe.
 *
 * The local wire types mirror the generated `AgentAvailability` / `AuthState` /
 * `CodingAgentKind` and exist only so the card can switch exhaustively on the
 * five-arm auth union: the generator flattens a serde-tagged enum into a single
 * object type with every field optional, which type-checks but lets a missed arm
 * pass silently. If a future generator version emits a discriminated union, delete
 * these and import the generated ones.
 *
 * It is deliberately NOT a raw `fetch`: the generated client already carries the
 * ephemeral `baseUrl` and the per-launch `X-Secret-Key` that `renderer.tsx` set on
 * it. A hand-rolled fetch would have to re-derive both, which is a second source
 * of truth for the daemon's address.
 */

/** serde snake_case of the `CodingAgentKind` enum. */
export type CodingAgentKind = 'claude_code' | 'codex';

/**
 * `AuthState`, internally tagged on `state`.
 *
 * `signed_in_with_api_key` is **not** an error: the CLI works, it just bills per
 * token. Rendering it as a failure is the specific mistake the variant exists to
 * prevent — see `unavailable_error` in
 * `crates/biorouter/src/providers/coding_agent/mod.rs`.
 */
export type CodingAgentAuth =
  | { state: 'not_installed' }
  | { state: 'signed_out' }
  | { state: 'signed_in_subscription'; plan?: string | null; account?: string | null }
  | { state: 'signed_in_with_api_key' }
  | { state: 'indeterminate'; detail: string };

/** One CLI's full situation. camelCase on the wire. */
export interface CodingAgentAvailability {
  kind: CodingAgentKind;
  /** `"claude_code"` | `"codex"` — the `BIOROUTER_PROVIDER` value. */
  providerId: string;
  /** `"Claude Agent"` | `"Codex"`. Never rendered from a literal here. */
  displayName: string;
  /** Absolute path of the resolved binary, or null when nothing was found. */
  path?: string | null;
  /** Raw first line of `--version`. */
  version?: string | null;
  auth: CodingAgentAuth;
  /** The command to run when `auth` says the user must act. */
  loginCommand: string;
  /** The command that installs the CLI. Shown, never executed — see the card. */
  installHint: string;
}

export interface CodingAgentStatusResponse {
  agents: CodingAgentAvailability[];
}

/**
 * Probe both CLIs.
 *
 * ⚠ Each call **spawns the vendor CLIs** (`--version` plus a credential probe),
 * which is why the card fetches on mount and on an explicit user action only, and
 * never polls.
 */
export async function fetchCodingAgentStatus(): Promise<CodingAgentStatusResponse> {
  const { data } = await codingAgentsStatus({ throwOnError: true });
  return data as CodingAgentStatusResponse;
}

/**
 * The provider config key each agent declares — one required key, with a default,
 * which is the only thing making a keyless provider report as configured.
 *
 * ⚠ Both halves are load-bearing. `check_provider_configured` treats a provider as
 * unconfigured until the key is **actually saved**, default or no default, so the
 * ready path must write it (the `LLAMACPP_PORT` move in
 * `LlamaServerInlineCard.tsx`). And the value written is the bare command name,
 * not the absolute `path` the probe resolved: `resolve_binary` honours this key and
 * then does its own augmented PATH search, so the bare name keeps working after an
 * nvm/volta/asdf switch moves the binary, where a pinned absolute path would break
 * silently. A user whose CLI lives somewhere unusual overrides the key in Settings.
 */
export const AGENT_COMMAND_CONFIG: Record<CodingAgentKind, { key: string; value: string }> = {
  claude_code: { key: 'CLAUDE_CODE_COMMAND', value: 'claude' },
  codex: { key: 'CODEX_COMMAND', value: 'codex' },
};

/**
 * The metered, per-token provider for the same vendor — the alternative offered to
 * a user who is signed in with an API key.
 *
 * Copy only, and a deliberate exception to "never name a provider in the renderer":
 * the daemon does not serve this pairing, and the sentence is useless without it
 * ("use the metered provider instead" — which one?). It names providers that are
 * registered built-ins, so it cannot dangle.
 */
export const METERED_SIBLING: Record<CodingAgentKind, string> = {
  claude_code: 'Anthropic',
  codex: 'OpenAI',
};
