/**
 * Issue #56 DR-16: the header that proves a request came from the person at the
 * keyboard rather than from the model.
 *
 * The daemon cannot tell the two apart on its own — `check_token` compares one
 * machine-wide bearer and has no principal, so every authenticated request looks
 * identical whoever sent it (AR-11/AR-15). Routes that must tell them apart
 * therefore require this header.
 *
 * ⚠ **Do not keep a count of them here.** This comment used to say "three
 * routes" and name them — the raise channels. By the time issue #56 Task 58
 * landed it was wrong in the number and in the kind, because reaching into a
 * private chat at all now needs the proof too. A stale invariant in the doc of a
 * security header is worse than no invariant, so the enumerations live where a
 * test fails when they drift:
 *
 * - raising a chat's privacy capability (DR-16) —
 *   `auth::tests::all_five_raise_channels_call_the_guard`;
 * - copying a private chat (DR-19) — `COPY_OF_PRIVATE_REFUSAL_MARKER` and its
 *   tests in `crates/biorouter-server/src/routes/session.rs`;
 * - reaching into a private chat at all (Task 58 / #47) — the gated-list table
 *   in `crates/biorouter-server/src/routes/session_reach.rs`.
 *
 * ⚠ It is attached PER REQUEST, never through `client.setConfig`.
 *
 * Not because it is rare: since Task 58 it rides on every `getSession` and every
 * `reply`, which is the renderer's busiest path. It is per request because a
 * default on the generated client rides on calls this module never sees — SSE
 * reconnects, polling loops, whatever a future feature adds — and the proof only
 * means anything while it is confined to calls a user gesture actually produced.
 * A default header would make it a second copy of the daemon secret.
 *
 * The renderer is the user's surface, so every call it makes is a user act. The
 * model reaches these same routes over HTTP without going through here, and that
 * is precisely the caller the header separates out.
 */
/**
 * Issue #56 DR-16. The substring that marks a 409 as *"this request carried no
 * proof it came from the user"*.
 *
 * ⚠ Mirrored verbatim from `USER_ACTION_REFUSAL_MARKER` in
 * `crates/biorouter/src/privacy/refusal.rs`, where a unit test asserts both
 * refusals the model picker can receive carry it. Change both together.
 */
export const USER_ACTION_REFUSAL_MARKER = "is the user's decision, not yours";

/**
 * Was this thrown value the daemon refusing a raise for want of a user-proof?
 *
 * Under `throwOnError` the generated @hey-api client throws the PARSED BODY, not
 * the response (`api/client/client.gen.ts`), so the 409 status never reaches the
 * catch arm and a substring is all there is to go on. A `typeof === 'string'`
 * test alone would not do: a 500 from the same routes also carries a plain-text
 * body, and reporting one as "your backend has no user-action key" would be a
 * confident lie. Gate A's refusal is a typed JSON object and is matched before
 * this.
 *
 * The user should ordinarily never see the toast this gates — the picker carries
 * the proof. It appears on a backend the user started themselves (open question
 * 23), which is why the message names that cause rather than accusing the person
 * at the keyboard of being a model.
 */
export const isUserActionRefusal = (error: unknown): boolean =>
  typeof error === 'string' && error.includes(USER_ACTION_REFUSAL_MARKER);

/**
 * Issue #56 DR-19. The substring that marks a 403 from a COPY handler
 * (`/sessions/{id}/diverge`, `/sessions/{id}/edit_message`) as *"branching this
 * private chat would mint another private-capability chat, and nothing proved
 * this came from you"*.
 *
 * ⚠ Mirrored verbatim from `COPY_OF_PRIVATE_REFUSAL_MARKER` in
 * `crates/biorouter-server/src/routes/session.rs`, where a unit test asserts the
 * refusal carries it. Change both together. The refusal itself is model-facing
 * prose and gets reworded; the marker is the contract.
 *
 * Deliberately a DIFFERENT marker from {@link USER_ACTION_REFUSAL_MARKER}, and
 * the Rust test asserts the two messages do not share one. The picker's refusal
 * is answered by "switch this chat's model"; this one by "branch it from the
 * chat window". A single toast for both would send the user somewhere that
 * cannot help.
 */
export const COPY_OF_PRIVATE_REFUSAL_MARKER = 'only the person at the keyboard may do it';

/**
 * Was this thrown value the daemon refusing to branch a private chat for want of
 * a user-proof?
 *
 * Same constraint as {@link isUserActionRefusal}: under `throwOnError` the
 * generated client throws the PARSED BODY, not the response, so the 403 status
 * never reaches the catch arm and a substring is all there is to go on. The
 * `typeof === 'string'` test is load-bearing in both directions — a 500 from the
 * same route also carries a plain-text body, and a real `Error` (a network
 * failure, a thrown assertion) is not a policy refusal however it reads.
 */
export const isPrivateCopyRefusal = (error: unknown): boolean =>
  typeof error === 'string' && error.includes(COPY_OF_PRIVATE_REFUSAL_MARKER);

export const userActionHeaders = async (): Promise<Record<string, string>> => {
  try {
    return { 'X-User-Action': await window.electron.getUserActionKey() };
  } catch {
    // An older preload, or a surface with no bridge at all. Sending nothing
    // fails closed at the daemon, which is the correct direction: the request
    // is refused and explained, not silently allowed.
    return {};
  }
};
