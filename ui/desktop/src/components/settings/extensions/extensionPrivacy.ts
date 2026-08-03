import type { ProviderTier, SessionClassification } from '../../../api/types.gen';
import { nameToKey } from './utils';

/**
 * The renderer's copy of the compiled-in private set (issue #56, R11).
 *
 * ⚠ **This is a mirror, not the source of truth.** Enforcement lives in
 * `crates/biorouter/src/privacy/registry_private.rs`
 * (`PRIVATE_EXTENSIONS`), read through `classify_extension`, and Gates C
 * (dispatch), E (discovery) and F (enable) all key on that. Nothing here can
 * grant or revoke access; this module exists only so the GUI can *say* what the
 * daemon is going to do, before the user hits it.
 *
 * There is no wire field to read instead — `GET /config/extensions` serves
 * `ExtensionEntry`, which carries no tier, and Task 8's OpenAPI-diff gate
 * deliberately froze that schema so a local `config.yaml` can never declare a
 * tier for itself (that would be R11(i) inverted). So the set is duplicated, and
 * the drift runs in the direction that under-marks: an extension added to the
 * Rust baseline and not here loses its GUI warning while staying fully
 * enforced. That is the safe half — the user sees no warning and then sees the
 * refusal — but it is still drift, and the two lists must be edited together.
 *
 * Task 37 replaces this constant with `PRIVATE_EXTENSIONS ∪
 * private(last_good_fetch)` off the BAAM registry. When it does, this function
 * is the single seam it has to change.
 *
 * Keys are `nameToKey` keys — whitespace-stripped and lower-cased — which is the
 * same reduction `name_to_key` applies in Rust, so `UCSFOMOPAgent`,
 * `ucsfomopagent` and ` ucsfomopagent ` all resolve identically on both sides.
 */
export const PRIVATE_EXTENSION_KEYS: readonly string[] = ['cdwagent', 'ucsfomopagent'];

/** R11(ii): anything not on the marketplace list is Public. Fail-open, by operator ruling. */
export function classifyExtension(name: string): ProviderTier {
  return PRIVATE_EXTENSION_KEYS.includes(nameToKey(name)) ? 'private' : 'public';
}

/**
 * §14.5's third state, as one predicate: *is this pairing refused?*
 *
 * The question a user needs answered is never "what is this extension" — a
 * static badge says that — but "will it work **here**". `config.yaml` enables
 * extensions globally with a single flag and there is no per-session
 * enablement, so a user who enables `ucsfomopagent` sees **Enabled** in
 * Settings while the tool is simply absent from every public-model chat.
 *
 * `undefined` for `callerTier` means *nobody could resolve it*, and that is
 * deliberately NOT treated as public. Walling a working tool on a failed read
 * is the same defect as omitting it — the user is told the tool is unavailable
 * when it is not — so an unresolved tier judges nothing at all.
 *
 * The rule matches `privacy_refusal` in
 * `crates/biorouter/src/privacy/refusal.rs` exactly: only a Private extension
 * under a Public caller is refused. A Public extension is callable from
 * anywhere, including a private chat.
 */
export function extensionPairingRefused(
  extensionName: string,
  callerTier: SessionClassification | ProviderTier | undefined
): boolean {
  if (callerTier === undefined) return false;
  return classifyExtension(extensionName) === 'private' && callerTier === 'public';
}
