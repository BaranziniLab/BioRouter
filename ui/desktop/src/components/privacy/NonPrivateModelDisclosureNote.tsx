import { useDisclosure } from './disclosureCopy';

/**
 * The one-line disclosure, for the surfaces that carry it permanently (issue
 * #56, DR-17 requirement 3) — today the provider grid's Commercial section.
 *
 * ⚠ **It reads no acknowledgement and never goes quiet.** That is what makes
 * "shown once, forcefully" a defensible design rather than a popup somebody
 * clicked past in week one: the blocking dialog is the introduction, and these
 * standing surfaces are the record.
 *
 * ⚠ **It does not read the master privacy switch** — see `disclosureCopy.ts` —
 * and it renders nothing at all if the copy cannot be fetched, rather than
 * falling back to a second English string of its own.
 */
export function NonPrivateModelDisclosureNote({ className }: { className?: string }) {
  const { copy } = useDisclosure();
  if (!copy) return null;
  return (
    <p data-testid="non-private-model-note" className={className}>
      {copy.short}
    </p>
  );
}
