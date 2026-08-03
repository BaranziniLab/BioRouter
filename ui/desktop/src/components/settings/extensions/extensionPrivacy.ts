import type { ProviderTier, SessionClassification } from '../../../api/types.gen';
import { learnedPrivateExtensionKeys } from '../../baam/privateSet';
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
 * This constant is the compiled-in HALF of the private set. Task 37 made
 * `classifyExtension` below the union `PRIVATE_EXTENSIONS ∪
 * private(last_good_fetch)`, and this function stayed the single seam — the
 * constant is still the floor nothing can lower.
 *
 * Keys are `nameToKey` keys — whitespace-stripped and lower-cased — which is the
 * same reduction `name_to_key` applies in Rust, so `UCSFOMOPAgent`,
 * `ucsfomopagent` and ` ucsfomopagent ` all resolve identically on both sides.
 */
export const PRIVATE_EXTENSION_KEYS: readonly string[] = ['cdwagent', 'ucsfomopagent'];

/**
 * R11(ii): anything not on the marketplace list is Public. Fail-open, by
 * operator ruling.
 *
 * The list is `PRIVATE_EXTENSION_KEYS ∪ private(last_good_fetch)` (§10.2), so an
 * extension marked private upstream after this build shipped is badged private
 * here from the next successful catalogue fetch onward, permanently. The union
 * only ever adds: a live document cannot take a badge off an extension this
 * build names, and cannot take one off an extension a previous fetch learned.
 *
 * The learned half is read synchronously off persisted storage rather than
 * awaited, because every caller is a render.
 *
 * ⚠ **The learned half can disagree with Rust, and in the over-marking
 * direction.** Rust's `classify_extension` reads the compiled baseline alone —
 * deliberately, and at length, in `crates/biorouter/src/privacy/extensions.rs`:
 * the last-good fetch is written and read entirely on the Electron side, and an
 * always-empty reader in Rust would read as enforcement that does not exist. So
 * the moment the site publishes a NEW private extension, this function badges it
 * Private and the daemon still treats it Public — until the app updates and the
 * generator rewrites the baseline. That is §15.4's first-run-after-upgrade
 * window, seen from the GUI side.
 *
 * The direction matters and is not the same as the constant's drift documented
 * above. That one under-marks: no warning, then a refusal, and the user is never
 * misled about what the daemon will do. This one over-marks: the user is told
 * about a wall that is not there yet. It is the safer direction for the user's
 * *behaviour* — they avoid pasting a cohort into a public chat, which is the
 * point of the badge — but it is still a promise the daemon is not yet keeping,
 * and it should not be discovered by someone puzzled that a Private-badged
 * extension answered a public model. Closing it means giving Rust a reader for
 * the last-good document, which is a one-line change at the `||` there.
 */
export function classifyExtension(name: string): ProviderTier {
  const key = nameToKey(name);
  if (PRIVATE_EXTENSION_KEYS.includes(key)) return 'private';
  return learnedPrivateExtensionKeys().has(key) ? 'private' : 'public';
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
