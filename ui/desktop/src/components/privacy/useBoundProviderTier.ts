import { useEffect, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { useModelAndProvider } from '../ModelAndProviderContext';
import type { ProviderTier } from '../../api/types.gen';

/**
 * The tier of the model this chat is bound to right now — the value Gate C
 * actually judges a tool call on (issue #56).
 *
 * ⚠ **This exists because the composer used to ask a DIFFERENT question than
 * the daemon, and got a different answer.** The extension selector judged its
 * pairings on the session's ratcheted `privacy_tier`; the daemon judges them on
 * `CallCapability::sample(&provider).tier()`, which is
 * `Provider::tier()` off the bound instance. Those two agree only *after* a turn
 * has run. A session is created `public` and the ratchet fires at the start of
 * the first turn (`agents/agent.rs`, `raise_privacy(f, "turn:<provider>")`), so
 * in every chat that had not yet run one — a brand-new chat, and every chat
 * predating the feature — a UCSF Versa model resolving **Private** was presented
 * as public: `ucsfomopagent` and `cdwagent` rendered disabled with "Unavailable
 * in this chat (public model)" while the daemon would have dispatched them
 * without complaint. Judging on this value instead makes the pre-flight state
 * what the gate will do.
 *
 * ⚠ **`resolved_tier`, never `metadata.tier`.** The metadata's tier is the
 * *type-level* claim — what the provider module ships — and is documented in
 * `providers/base.rs` as "do not hang a badge on this field", because a
 * re-pointed `ollama` still ships `private` there while its instance resolves
 * `public`. `resolved_tier` comes off the live instance `providers::create`
 * built, in the same call that produced the row's affiliation, so the two axes
 * are one sample of one endpoint.
 *
 * ⚠ **`undefined` is "not resolved", and it must never be read as public.**
 * It is the answer for an unconfigured provider, a construction failure, a
 * timeout, and a payload that predates the field. `extensionPairingRefused`
 * treats `undefined` as *judge nothing*, which is the direction that costs a
 * warning rather than walling a working tool — the same failure this hook
 * exists to remove, inverted. Failing an unresolvable private model over to
 * "public" is precisely the wrong direction.
 *
 * ⚠ **Read-only.** Nothing here is ever sent back to the daemon; enforcement
 * lives in the Rust gates and this only lets the GUI *say* what they will do.
 */
export function useBoundProviderTier(): ProviderTier | undefined {
  const { currentProvider } = useModelAndProvider();
  const { getProviders } = useConfig();
  const [tier, setTier] = useState<ProviderTier | undefined>(undefined);

  useEffect(() => {
    if (!currentProvider) {
      setTier(undefined);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const rows = await getProviders(false);
        const row = rows.find((candidate) => candidate.name === currentProvider);
        if (!cancelled) setTier(readResolvedProviderTier(row));
      } catch {
        // A provider catalog we cannot read is not evidence of a tier. Say
        // nothing rather than assert one — and in particular do not assert the
        // permissive one.
        if (!cancelled) setTier(undefined);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentProvider, getProviders]);

  return tier;
}

/**
 * The instance-resolved tier carried by one `ProviderDetails` row, or
 * `undefined` when there is none.
 *
 * ⚠ **Parsed structurally rather than typed off `src/api`**, for the same
 * reason `readProviderAffiliation` is. The generated client now carries the
 * field, but the client describes the daemon this tree was built against, not
 * the one answering: the desktop app talks to whichever `biorouterd` is
 * running, and a lagging one serves rows with no `resolved_tier` at all. A
 * total parse over a wider type reads that as *unresolved* — which is the
 * fail-safe answer — where a typed read would hand `undefined` through the same
 * path with the type system claiming it could not happen.
 *
 * ⚠ **An unrecognised tier is `undefined`, never a default member.** A third
 * tier the daemon learns before this renderer does must read as *unresolved*
 * rather than as whichever of the two the parser fell back to — falling back to
 * `'public'` would wall every private extension under it, and falling back to
 * `'private'` would silently promise reach the gate will refuse.
 */
export function readResolvedProviderTier(row: unknown): ProviderTier | undefined {
  if (typeof row !== 'object' || row === null) return undefined;
  const raw = (row as { resolved_tier?: unknown }).resolved_tier;
  return raw === 'public' || raw === 'private' ? raw : undefined;
}
