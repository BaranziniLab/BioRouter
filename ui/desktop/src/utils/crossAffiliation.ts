/**
 * The user's half of DR-26 (issue #56, Task 57): accepting one stated
 * cross-institutional data flow.
 *
 * DR-26 rules that a mismatch **warns** and that the user may proceed if they
 * insist — a blocked-outright design is one researchers route around by turning
 * the feature off, and legitimate cross-institutional work under a real DUA
 * exists. The daemon has held up its end since Task 49:
 * `POST /agent/cross_affiliation_grant` records the acceptance behind the
 * `X-User-Action` header, and Gate C consults it before composing a refusal at
 * all. What was missing was the second half of the sentence — the refusal
 * arrived, the model was told to ask the human, and the human had no button.
 * Their only route past it was to switch the chat's model, which is a hard block
 * wearing a warning's clothes.
 *
 * ⚠ **The agent must not be able to press it.** DR-19 is unchanged: the model
 * may ask, only the user may answer. That is why the grant call lives here:
 * `the generated grant call is named by exactly one renderer module` in the
 * sibling test pins this file as the only place outside `src/api/` that may name
 * it — the renderer's half of the Rust audit that caps the proof-of-user at one
 * construction site.
 *
 * ⚠ **What that audit does NOT pin**, said plainly because review read the
 * earlier wording as promising it: how many callers
 * {@link acceptCrossAffiliationFlow} has. A second component importing it from
 * here would pass the audit untouched. Today there is exactly one importer and
 * its only call is an `onClick` — a fact about the tree, not something a test
 * holds — and a second one deserves the same scrutiny as the first, because
 * what it posts is a person's acceptance of a data flow.
 *
 * ⚠ **What this module reads is attacker-controlled, and that is new here.**
 * {@link crossAffiliationOffer} parses a **tool result's error text**, which a
 * third-party MCP server composes in full. This is the first control in the
 * privacy feature drawn out of tool-produced prose — the {@link
 * isUserActionRefusal} precedent it is modelled on reads the renderer's own HTTP
 * response (`ModelAndProviderContext.tsx`), not tool output — so a rogue
 * extension can make an "Approve this flow for X" button appear beneath a
 * warning it wrote itself. Two things bound it, and both were checked against
 * the daemon rather than assumed:
 *
 * 1. `agent_cross_affiliation_grant` re-derives the mismatch from the live agent
 *    (`Agent::cross_affiliation_grant_subject`, `routes/agent.rs`) and answers
 *    **400** when there is none — so no acceptance can be conjured for a flow
 *    that is not genuinely refused, whatever the prose claimed.
 * 2. The confirmation the user then reads is the daemon's own
 *    `accepted_statement`, composed from the warning the *daemon* re-derived and
 *    not from anything sent — so a spoofed pretext is exposed after the press.
 *
 * The residual is therefore **inducement, not forgery**: a rogue extension can
 * invite a press it cannot fake the effect of. ⚠ Do not add a branch that trusts
 * this text for anything beyond *which extension key to put in the request* —
 * the key is the one thing the daemon independently re-checks, and it is what
 * keeps the parse honest.
 *
 * ⚠ Its own module rather than `utils/userAction.ts`. That file is about the
 * *header* that proves a request came from the keyboard, and the two refusal
 * markers that key off a missing one. This is a different subject — a boundary
 * the user may accept rather than a proof they failed to supply — and it carries
 * an API call and a policy mode, neither of which belongs beside a header
 * helper. It reuses {@link userActionHeaders} rather than re-deriving it.
 */
import { agentCrossAffiliationGrant, readConfig } from '../api';
import { userActionHeaders } from './userAction';

/**
 * Issue #56, DR-26. The frame that marks a cross-affiliation refusal as one a
 * grant can clear, and that carries the extension key the grant names.
 *
 * ⚠ Mirrored verbatim from `CROSS_AFFILIATION_ACCEPT_MARKER` in
 * `crates/biorouter/src/privacy/refusal.rs`, where
 * `only_the_refusal_a_grant_can_clear_offers_the_accept_marker` pins both the
 * marker and the backticked key that must follow it. Change both together.
 *
 * A substring is all there is to go on: the wire form of a failed tool call is
 * `{status, error, error_kind, retryable}` (`conversation::tool_result_serde`),
 * which keeps the message text and **drops** `ErrorData::data`. So there is no
 * structured channel to put an extension key on, and the key rides in the prose.
 */
export const CROSS_AFFILIATION_ACCEPT_MARKER = 'The flow they would be approving is ';

/**
 * What accepting commits the user to, shown BEFORE they press.
 *
 * ⚠ Mirrored verbatim from `GRANT_SCOPE_COPY` in
 * `crates/biorouter/src/privacy/grant.rs`, whose own doc comment is explicit
 * that this sentence belongs to "the dialog the user clicks" and that
 * hand-written copies drift within one release. It cannot simply be fetched:
 * the daemon composes it into `accepted_statement` and returns it only *after*
 * the grant is recorded, which is one press too late for the person deciding.
 * So it is a mirror, and
 * `privacy::grant::tests::the_scope_copy_the_user_reads_is_the_one_the_daemon_records`
 * reads THIS file and fails the Rust build if the two ever differ.
 *
 * ⚠ One line, deliberately. A `+`-concatenated or template-wrapped spelling
 * would still read correctly and would defeat that detector, which compares
 * bytes.
 */
export const GRANT_SCOPE_COPY =
  "Approving records your acceptance for this chat, this extension and this model's institution only. It is not remembered for other chats, for other extensions, or if you switch this chat to a model covered by a different institution's agreements — each of those is a different data flow and would be asked again.";

/**
 * What the user would be accepting: one extension, in this chat, on the model it
 * is bound to right now.
 *
 * ⚠ There is deliberately no affiliation here, matching
 * `CrossAffiliationGrantRequest`. The institution is read by the daemon from the
 * same sample that produced the warning; a client-supplied one would let a
 * caller record an acceptance for a triple the user was never shown.
 */
export interface CrossAffiliationOffer {
  /** The extension key exactly as Gate C spelled it when it refused. */
  extension: string;
}

/**
 * Is this failed tool call a cross-institutional refusal the user can accept,
 * and if so, which extension?
 *
 * `null` for every other failure, and that is the important half: two of the
 * three sites that compose this refusal never consult the grant
 * (`assert_extension_reachable`, which has no session to key one on, and the
 * agent's own enable path, which refuses by design), so their refusals carry no
 * frame. An accept control there would record a real acceptance and leave the
 * retry refused — the bug this fixes, inverted.
 *
 * ⚠ The `typeof === 'string'` test is load-bearing in both directions, exactly as
 * in {@link isUserActionRefusal}: the tool result's `error` is prose, and a
 * thrown `Error` or a structured payload that happens to read the same way is
 * not a refusal.
 */
export const crossAffiliationOffer = (toolError: unknown): CrossAffiliationOffer | null => {
  if (typeof toolError !== 'string') return null;
  const at = toolError.indexOf(CROSS_AFFILIATION_ACCEPT_MARKER);
  if (at === -1) return null;
  const tail = toolError.slice(at + CROSS_AFFILIATION_ACCEPT_MARKER.length);
  // The key is backtick-delimited and immediately follows the frame. Anything
  // else is a reworded or truncated refusal, and guessing at one would post a
  // grant keyed on a name the daemon's `name_to_key` turns into a row no lookup
  // ever matches — a control that silently does nothing.
  const match = /^`([^`]+)`/.exec(tail);
  const extension = match?.[1]?.trim();
  return extension ? { extension } : null;
};

/**
 * DR-27's mixing policy, as the renderer sees it.
 *
 * - `open` — institutional mixing is not policed on this machine, so no mismatch
 *   is raised and there is nothing to accept.
 * - `standard` — the in-app proof (`X-User-Action`) is what accepting takes.
 * - `strict` — accepting additionally requires DR-20's system-level prompt.
 */
export type MixingMode = 'open' | 'standard' | 'strict';

/**
 * The config key that holds the mixing policy.
 *
 * ⚠ **Nothing writes it yet.** The setting itself is Task 52's, and until it
 * lands this key is absent on every machine and {@link mixingModeFromConfig}
 * answers `standard` — which is the mode the daemon has always enforced, so the
 * behaviour on a real machine today is exactly what it was. This is a live read
 * of the daemon's config rather than a placeholder, so wiring the setting is a
 * matter of writing the key; but the SPELLING is this task's guess and must be
 * reconciled against Task 52's daemon-side constant when it arrives, in the same
 * way `PRIVACY_TIERS_KEY` is one spelling shared with
 * `biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY`.
 */
export const MIXING_POLICY_KEY = 'BIOROUTER_PRIVACY_MIXING_POLICY';

/**
 * ⚠ **An absent or unreadable value is `standard`, never `open` and never
 * `strict`.**
 *
 * `open` would silently withdraw the control — the very failure this task
 * exists to fix — and `strict` would silently withdraw the machine's only route
 * past a warning, which is the failure DR-26 exists to prevent. `standard` is
 * the documented default and the only safe answer to "the setting is not there".
 *
 * The `trim()`/`toLowerCase()` mirror `privacyTiersEnabledFromConfig`'s rule for
 * the same reason it has one: without the same normalisation on both sides, a
 * hand-edited value renders one policy in the app while the daemon enforces
 * another, and the user is told something false about a control they just used.
 */
export const mixingModeFromConfig = (value: unknown): MixingMode => {
  if (typeof value !== 'string') return 'standard';
  const v = value.trim().toLowerCase();
  return v === 'open' || v === 'strict' ? v : 'standard';
};

/** Read the mixing policy from the daemon's config. Failures read `standard`. */
export const readMixingMode = async (): Promise<MixingMode> => {
  try {
    const response = await readConfig({
      body: { key: MIXING_POLICY_KEY, is_secret: false },
    });
    return mixingModeFromConfig(response.data);
  } catch {
    // A daemon that cannot answer must not decide the policy by failing. See
    // `mixingModeFromConfig` for why the fallback is `standard` in both
    // directions.
    return 'standard';
  }
};

/**
 * Record the user's acceptance of one cross-institutional flow, and return the
 * exact statement the daemon says was accepted.
 *
 * ⚠ **This is the ONE call site of the generated grant client outside
 * `src/api/`**, and the audit in the sibling test is what keeps it that way. The
 * `X-User-Action` header is attached PER REQUEST and never through
 * `client.setConfig` — see `utils/userAction.ts` for why a default header would
 * make the proof a second copy of the daemon secret.
 *
 * The response's `accepted` is the daemon's own composition (`grant::
 * accepted_statement`: the warning the user was shown plus `GRANT_SCOPE_COPY`),
 * so the confirmation the user reads and the sentence the daemon recorded cannot
 * differ by a word. Returning it rather than paraphrasing is the point.
 */
export const acceptCrossAffiliationFlow = async (
  sessionId: string,
  extension: string
): Promise<string> => {
  const response = await agentCrossAffiliationGrant({
    body: { session_id: sessionId, extension },
    headers: await userActionHeaders(),
    throwOnError: true,
  });
  return response.data.accepted;
};
