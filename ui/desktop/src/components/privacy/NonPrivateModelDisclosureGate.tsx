import { useEffect, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { NonPrivateModelDisclosure } from './NonPrivateModelDisclosure';
import {
  disclosureRequiredForTier,
  useDisclosure,
  useSoleDisclosurePresenter,
} from './disclosureCopy';

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
 * ⚠ **Before, not after — and precisely how far before.** Once the dialog is up
 * it is modal: Radix traps focus and marks the rest of the page `aria-hidden`,
 * so the composer behind it cannot be reached and nothing can be sent. An
 * acknowledgement collected once the transcript already went out is a receipt,
 * not a disclosure.
 *
 * What it does NOT do is *sequence* the composer. Between this mounting and the
 * daemon answering — one round trip, since the two fetches are parallel and the
 * store usually already holds the copy — the composer is live, so a user who
 * types and sends inside that interval sends before reading. The interval is
 * reachable only on the very first public bind of an install (afterwards the
 * record says acknowledged and no dialog is due at all), and the alternative —
 * holding the composer inert until a network answer arrives — would freeze the
 * app for every user on every chat against a daemon that is slow or gone. That
 * is a worse failure than the one it prevents, so the residual is written down
 * here rather than traded for it.
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

  // ⚠ Keyed on "a provider is bound", NOT on "it turned out to need this", so
  // the two fetches run in PARALLEL. Gating the copy on the registry lookup made
  // them serial and doubled the interval between mounting and being able to say
  // anything — see this component's doc comment on what that interval is. The
  // cost is one small GET on a machine that only ever runs local models, and the
  // store makes it once per app rather than once per pane.
  //
  // `dismissedUnrecorded` is set only when the daemon REFUSED to record the
  // acknowledgement. There is nothing the person at the keyboard can present to
  // satisfy a daemon that holds no user-action key, so once they have been told
  // it was not saved they are let past — and, because nothing was written, told
  // again next launch. It is shared across panes for the same reason
  // `acknowledged` is: otherwise the pane they got past releases the dialog to
  // the next one waiting for it.
  const {
    copy,
    acknowledged,
    acknowledge,
    acknowledgeError,
    dismissedUnrecorded,
    dismissUnrecorded,
  } = useDisclosure(Boolean(providerName));

  const wantsDialog =
    resolved?.required === true && acknowledged === false && copy !== null && !dismissedUnrecorded;
  // ⚠ ONE dialog, however many panes are open. `ChatGroupsShell` mounts one
  // `BaseChat` — and so one of these — per chat group, six at a time; without
  // this a two-pane split stacked two un-dismissible modals. The store makes the
  // panes agree about the acknowledgement; this makes exactly one of them show
  // it. Hooks are unconditional: this is called on every render, wanting or not.
  const isPresenter = useSoleDisclosurePresenter(wantsDialog);

  const open = wantsDialog && isPresenter;
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
          dismissUnrecorded();
          return;
        }
        setBusy(true);
        void acknowledge().finally(() => setBusy(false));
      }}
    />
  );
}
