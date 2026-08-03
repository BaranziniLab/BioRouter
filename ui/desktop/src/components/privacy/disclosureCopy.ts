import { useCallback, useEffect, useState } from 'react';
import { ackPrivacyDisclosure, getPrivacyDisclosure } from '../../api';
import type { ProviderTier } from '../../api';
import { userActionHeaders } from '../../utils/userAction';

/**
 * The non-private-model disclosure, as the renderer sees it (issue #56, DR-17
 * requirement 3).
 *
 * ⚠ **The renderer holds no copy of its own.** The one definition lives in
 * `crates/biorouter/src/privacy/disclosure.rs` and arrives over
 * `GET /privacy/disclosure`. Four hand-written copies of a sentence drift within
 * one release and the drifted one is always the one a user reads — so every
 * surface in this app (the dialog, the settings panel, the provider grid, the
 * model chip) renders what this module was handed, and a hardcoded English
 * string in any of them is the defect Step 5's gate greps for.
 *
 * ⚠ **Nothing here reads the master privacy switch.** DR-15 turns off gates, the
 * ratchet and refusals; it does not turn off the truth, and with enforcement off
 * the exposure is *larger*, not smaller. Every other privacy surface in this
 * folder reads the switch, which is exactly why wiring this one the same way is
 * the plausible mistake.
 */
export interface DisclosureCopy {
  /** The dialog heading, with `{provider}` still in it. */
  titleTemplate: string;
  /** The long form: the blocking dialog and the settings panel. */
  long: string;
  /** The one-line form: the model chip's tooltip, the provider grid. */
  short: string;
}

export interface DisclosureState {
  /** `null` until the copy has been fetched. */
  copy: DisclosureCopy | null;
  /** `null` until the daemon has answered. */
  acknowledged: boolean | null;
  /** Record the acknowledgement, carrying DR-16's proof of user. */
  acknowledge: () => Promise<void>;
}

/**
 * The heading for `providerDisplayName`, from the served template.
 *
 * The substitution lives here rather than in the Rust route so that one fetched
 * copy serves every provider a user switches between without a round trip.
 */
export function disclosureTitle(copy: DisclosureCopy, providerDisplayName: string): string {
  return copy.titleTemplate.replace('{provider}', providerDisplayName);
}

/**
 * Must the user be told what this provider can reach?
 *
 * The tier, and only the tier — mirroring `disclosure::required_for`. Task 5
 * owns the private set, so a fourth private provider must switch this off with
 * no edit here.
 *
 * ⚠ Called only with metadata that has actually RESOLVED. An absent `tier` on a
 * resolved entry is Public by the daemon's own polarity (`#[serde(default)]`
 * yields the fail-safe tier), so it discloses; "we have not looked yet" is
 * represented by having no metadata to pass, not by `undefined` here.
 */
export function disclosureRequiredForTier(tier: ProviderTier | null | undefined): boolean {
  return tier !== 'private';
}

/**
 * Fetch the copy and the acknowledgement state once per mount.
 *
 * A failed fetch leaves `copy` null and `acknowledged` null, and every surface
 * renders nothing rather than inventing prose. That is the one honest answer:
 * a blocking dialog with no text in it discloses nothing and only takes the app
 * away from the user.
 */
export function useDisclosure(enabled: boolean = true): DisclosureState {
  const [copy, setCopy] = useState<DisclosureCopy | null>(null);
  const [acknowledged, setAcknowledged] = useState<boolean | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    (async () => {
      try {
        const result = await getPrivacyDisclosure();
        const served = result?.data;
        if (cancelled || !served) return;
        setCopy({
          titleTemplate: served.title_template,
          long: served.long,
          short: served.short,
        });
        setAcknowledged(served.acknowledged);
      } catch {
        // Leave both null. See this function's doc comment.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  const acknowledge = useCallback(async () => {
    // DR-16's proof of user. Without it the daemon refuses with a 403, which is
    // the correct direction: a model must not be able to dismiss this on the
    // user's behalf.
    await ackPrivacyDisclosure({ headers: await userActionHeaders() });
    setAcknowledged(true);
  }, []);

  return { copy, acknowledged, acknowledge };
}
