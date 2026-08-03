import { useEffect, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { NonPrivateModelDisclosure } from './NonPrivateModelDisclosure';
import { disclosureRequiredForTier, useDisclosure } from './disclosureCopy';

export interface NonPrivateModelDisclosureGateProps {
  /**
   * The provider bound to the chat this gate sits in, or `null` before one is.
   * `null` shows nothing and fetches nothing: there is no model yet to disclose
   * anything about.
   */
  providerName?: string | null;
}

/** What we managed to learn about the bound provider. */
interface ResolvedProvider {
  displayName: string;
  required: boolean;
}

/**
 * Shows the disclosure the first time a public-tier provider is bound on this
 * install, before the first turn on it (issue #56, DR-17 requirement 3).
 *
 * ⚠ **Before, not after.** The dialog is modal — Radix traps focus and marks the
 * rest of the page `aria-hidden` — so the composer behind it cannot be reached
 * while it is up. An acknowledgement collected once the transcript already went
 * out is a receipt, not a disclosure.
 *
 * ⚠ **Once per install, not once per session.** A confirmation a user sees daily
 * is a confirmation they stop reading, and this one has no *action* to gate. So
 * it is shown once, forcefully, and is then carried permanently by the model
 * chip's tooltip, the provider grid's Commercial section and Settings → Privacy
 * — surfaces that read no acknowledgement at all and never go quiet.
 *
 * ⚠ **Nothing here reads the master privacy switch.** See `disclosureCopy.ts`.
 */
export function NonPrivateModelDisclosureGate({
  providerName,
}: NonPrivateModelDisclosureGateProps) {
  const { getProviders } = useConfig();
  const [resolved, setResolved] = useState<ResolvedProvider | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!providerName) {
      setResolved(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const providers = await getProviders(false);
        if (cancelled) return;
        const match = providers.find((p) => p.name === providerName);
        // A name with no registry entry is a provider Biorouter cannot vouch
        // for; an entry whose tier is absent is Public by the daemon's own
        // polarity. Both disclose — fail-safe here means fail towards telling
        // the user, because the cost of a redundant dialog is an annoyance and
        // the cost of skipping it is the misrepresentation DR-17 forbids.
        setResolved({
          displayName: match?.metadata?.display_name || providerName,
          required: disclosureRequiredForTier(match?.metadata?.tier),
        });
      } catch {
        if (!cancelled) {
          setResolved({ displayName: providerName, required: true });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [providerName, getProviders]);

  // The copy is fetched only once there is something to disclose, so a machine
  // that only ever runs local models never asks for it.
  const { copy, acknowledged, acknowledge, acknowledgeError } = useDisclosure(
    resolved?.required === true
  );

  // Set only when the daemon REFUSED to record the acknowledgement. There is
  // nothing the person at the keyboard can present to satisfy a daemon that
  // holds no user-action key, so once they have been told it was not saved they
  // are let past — and, because nothing was written, told again next launch.
  const [dismissedUnrecorded, setDismissedUnrecorded] = useState(false);

  const open =
    resolved?.required === true &&
    acknowledged === false &&
    copy !== null &&
    !dismissedUnrecorded;
  if (!open || !resolved || !copy) return null;

  return (
    <NonPrivateModelDisclosure
      open
      providerDisplayName={resolved.displayName}
      copy={copy}
      busy={busy}
      acknowledgeError={acknowledgeError}
      onAcknowledge={() => {
        if (acknowledgeError) {
          setDismissedUnrecorded(true);
          return;
        }
        setBusy(true);
        void acknowledge().finally(() => setBusy(false));
      }}
    />
  );
}
