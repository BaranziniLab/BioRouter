import { useEffect, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { useModelAndProvider } from '../ModelAndProviderContext';
import { readProviderAffiliation, type ProviderAffiliation } from './providerAffiliation';

/**
 * DR-26's third axis for the model this chat is bound to right now (issue #56).
 *
 * ⚠ **It reads the `ProviderDetails` ROW, not `row.metadata`.** The affiliation
 * is served beside the metadata rather than inside it precisely because
 * `ProviderMetadata` is the *type-level* claim — its own `tier` field carries
 * the warning "do not hang a badge on this field", since a re-pointed `ollama`
 * still ships `Private` there while its instance resolves Public. The daemon
 * resolves this field from a live instance through `providers::create`, the same
 * call `POST /agent/update_provider` makes before reading
 * `new_provider.affiliation()`, so a Versa module repointed elsewhere loses
 * Private and `ucsf` together and this hook sees the loss.
 *
 * ⚠ **`null` while unresolved, and `null` for a public model — both render
 * nothing.** Saying nothing is the only safe answer to "we have not looked yet",
 * and it is also the correct answer for a public model, which has no third axis
 * at all. The state that would be lost by guessing — `unstated`, a private model
 * naming no institution — is never produced by a failure, only by a successful
 * resolution, so the two cannot be confused.
 *
 * ⚠ **Read-only.** Nothing here is ever sent back. The grant route reads the
 * institution from its own sample; a client-supplied one would let a caller
 * record an acceptance for a triple the user was never shown.
 */
export function useBoundAffiliation(): ProviderAffiliation | null {
  const { currentProvider } = useModelAndProvider();
  const { getProviders } = useConfig();
  const [affiliation, setAffiliation] = useState<ProviderAffiliation | null>(null);

  useEffect(() => {
    if (!currentProvider) {
      setAffiliation(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const rows = await getProviders(false);
        const row = rows.find((candidate) => candidate.name === currentProvider);
        if (!cancelled) setAffiliation(readProviderAffiliation(row));
      } catch {
        // A provider catalog we cannot read is not evidence of an affiliation.
        // Say nothing rather than assert one.
        if (!cancelled) setAffiliation(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentProvider, getProviders]);

  return affiliation;
}
