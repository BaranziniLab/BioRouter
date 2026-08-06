/**
 * DR-26's warn-and-proceed statement, as the renderer shows it (issue #56).
 *
 * The ruling is that a cross-institutional mismatch **warns the user, naming
 * both institutions, before proceeding**. Three surfaces raise one, and until
 * this module existed only one of them told anybody:
 *
 * 1. **Dispatch** — Gate C refuses the tool call, the refusal prose carries the
 *    warning, and {@link ../components/privacy/CrossAffiliationAcceptCard}
 *    renders the accept control under it. This one worked.
 * 2. **Bind** — `Agent::update_provider` detected the mismatch and wrote it to
 *    `tracing::warn!`. The user switching models saw a green success toast.
 * 3. **User enable** — `POST /agent/add_extension` did the same. A researcher
 *    attaching another institution's connector was told nothing at all.
 *
 * Both (2) and (3) now return the daemon's statement in their 200 body, and this
 * module is the single place the renderer turns that body into something a
 * person reads.
 *
 * ⚠ **The words are the daemon's, never this file's.** Every warning is composed
 * once in `privacy::affiliation` and shipped verbatim; this module adds a title
 * and a container and nothing else. That is the whole reason it is one module
 * with one presenter rather than a snippet inlined at each call site — the
 * failure being repaired is precisely two surfaces describing one boundary
 * differently, and a second paraphrase here would recreate it inside the
 * renderer.
 *
 * ⚠ **Its own module rather than `utils/crossAffiliation.ts`**, which is about
 * the *accept control*: a refusal the user may clear, an API call that records
 * their acceptance, and a policy mode. This is a different subject — a statement
 * about something that already happened, with nothing to press — and it pulls in
 * the toast surface, which that module deliberately does not depend on.
 *
 * ⚠ **What is deliberately NOT here: an accept button.** A grant clears the
 * TOOL-CALL door and nothing else (`ExtensionManager::cross_affiliation_denial`
 * is the one production reader of `privacy::grant::is_granted`), so at a bind or
 * an enable there is no refusal to clear — the bind bound, the extension
 * attached. Offering "approve" here would ask the user to pre-authorise a flow
 * nobody has attempted yet, which is the exact thing
 * `CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT` exists to refuse in the daemon.
 * The control belongs where the refusal is, and it is already there.
 */
import { toastWarning } from '../toasts';

/**
 * What separates one warning from the next in a notice body.
 *
 * ⚠ Mirrored from `Agent::CROSS_AFFILIATION_NOTICE_SEPARATOR` in
 * `crates/biorouter/src/agents/agent.rs`, and pinned from the Rust side by
 * `agents::agent::gate_c_dispatch_tests::the_renderer_splits_the_notice_the_daemon_joins`
 * — the same shape as `GRANT_SCOPE_COPY`'s detector, and for the same reason: a
 * drift here does not fail, it silently renders two warnings as one paragraph.
 *
 * A blank line rather than a newline, because each warning is a full sentence
 * naming two institutions and a run-together pair reads as one confused claim
 * about three.
 */
export const CROSS_AFFILIATION_NOTICE_SEPARATOR = '\n\n';

/**
 * Split a `/agent/update_provider` or `/agent/add_extension` 200 body into the
 * warnings it carries. `[]` for every body that carries none.
 *
 * ⚠ **An empty body is the NORMAL answer and must never render anything** —
 * every public model, every local model, every chat whose connectors and model
 * share an institution, and every machine on DR-27's `open` policy come back
 * empty. A presenter that showed something for `''` would fire on essentially
 * every model switch in the product and train the user to dismiss the one that
 * matters.
 *
 * ⚠ The `typeof === 'string'` test is load-bearing in both directions, exactly as
 * in `crossAffiliationOffer`: a generated client whose response shape changes,
 * or a caller handing us an object, must read as "nothing to say" rather than
 * stringifying into a toast full of `[object Object]`.
 */
export const crossAffiliationNotices = (body: unknown): string[] => {
  if (typeof body !== 'string') return [];
  return body
    .split(CROSS_AFFILIATION_NOTICE_SEPARATOR)
    .map((notice) => notice.trim())
    .filter((notice) => notice.length > 0);
};

/**
 * Show the daemon's cross-institutional statement to the person who just bound a
 * model or attached a connector. Returns how many warnings were shown, so a
 * caller (and a test) can tell "nothing to say" from "not wired up".
 *
 * ⚠ **`autoClose: false`.** Every other toast in the app expires; this one is a
 * privacy statement about a data flow the user has just opened, and a statement
 * that vanishes after five seconds while the user is reading the model picker is
 * one they were not shown. They dismiss it themselves.
 *
 * ⚠ **One toast per warning, not one joined blob.** A bind can mismatch several
 * connectors at once, each naming a different pair of institutions, and folding
 * them into a single message loses which sentence is about which connector.
 *
 * ⚠ **No title, deliberately.** Both composers in `privacy::affiliation`
 * (`compose_mismatch`, `compose_unstated`) already open with *"Cross-institutional
 * data flow."*, so a heading would be the daemon's own first clause said twice —
 * and the only way to keep a heading without repeating it is to invent one here,
 * which is the second description of one boundary this fix exists to prevent.
 * `toastWarning` renders a message with no title (`toasts.tsx`: "title AND msg
 * render independently — a message with no title is no longer dropped"), so what
 * the user reads is the daemon's paragraph and nothing else.
 */
export const showCrossAffiliationNotice = (body: unknown): number => {
  const notices = crossAffiliationNotices(body);
  for (const msg of notices) {
    toastWarning({ msg, toastOptions: { autoClose: false } });
  }
  return notices.length;
};
