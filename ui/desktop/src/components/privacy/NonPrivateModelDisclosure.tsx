import { useRef } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';
import { disclosureTitle, type DisclosureCopy } from './disclosureCopy';
import { DisclosureProse } from './DisclosureProse';

export interface NonPrivateModelDisclosureProps {
  open: boolean;
  /** The provider the user is about to talk to, as a person would name it. */
  providerDisplayName: string;
  /**
   * The served copy. **Required** — there is no fallback string in this file,
   * and that is the point: a default here would be a second definition of the
   * sentence, and it would be the one that shipped stale.
   */
  copy: DisclosureCopy;
  onAcknowledge: () => void;
  busy?: boolean;
  /**
   * Why the acknowledgement was not recorded, when it was not.
   *
   * ⚠ Shown, and the button then says so. A daemon holding no user-action key
   * refuses this and there is nothing the person at the keyboard can present to
   * satisfy it, so the honest sequence is: tell them it was not saved, and let
   * them past — this dialog conveys a fact, it does not grant a permission.
   * Silently closing as though it had been saved is the failure that turns a
   * once-per-install disclosure into a modal they see on every launch and stop
   * reading.
   */
  acknowledgeError?: string | null;
}

/**
 * The one-time blocking disclosure (issue #56, DR-17 requirement 3).
 *
 * ⚠ **It cannot be dismissed.** No Escape, no overlay click, no close button —
 * `dismissible={false}` on `DialogContent` prevents the first two and suppresses
 * the third. A disclosure a user can flick away without reading is a disclosure
 * that did not happen, and this one has no *action* to gate: there is no safer
 * alternative on offer at this moment, only a fact to convey.
 *
 * ⚠ **No key acknowledges it.** Task 29's `DangerousConfirmDialog` established
 * the discipline and the reason is the same here: "type, Enter" ends every other
 * dialog in this app, so a dialog whose whole job is to be *read* must not be
 * dismissible by the muscle memory that skips reading. There is no `<form>`, the
 * button is `type="button"`, and initial focus is put on the dialog's body
 * rather than on the button — so the first Enter after it opens lands on
 * nothing.
 *
 * ⚠ **Every word comes from {@link copy}.** This component contains no product
 * prose. See `disclosureCopy.ts`.
 */
export function NonPrivateModelDisclosure({
  open,
  providerDisplayName,
  copy,
  onAcknowledge,
  busy = false,
  acknowledgeError = null,
}: NonPrivateModelDisclosureProps) {
  const bodyRef = useRef<HTMLDivElement>(null);

  return (
    <Dialog open={open}>
      <DialogContent
        data-testid="non-private-model-disclosure"
        dismissible={false}
        className="sm:max-w-[560px]"
        // Focus lands on the prose, not on the way out of it. Radix's default
        // would focus the first focusable node, which here is the only button —
        // and an acknowledgement one keystroke away from the dialog opening is
        // the receipt this task exists to avoid.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          bodyRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{disclosureTitle(copy, providerDisplayName)}</DialogTitle>
        </DialogHeader>

        <div
          ref={bodyRef}
          tabIndex={-1}
          className="space-y-3 text-sm text-text-default outline-none max-h-[50vh] overflow-y-auto"
        >
          <DisclosureProse text={copy.long} paragraphClassName="min-w-0 [overflow-wrap:anywhere]" />
        </div>

        {acknowledgeError && (
          <p
            data-testid="disclosure-ack-error"
            className="min-w-0 [overflow-wrap:anywhere] text-sm text-text-muted"
          >
            {acknowledgeError}
          </p>
        )}

        <div className="flex justify-end pt-2">
          <Button type="button" variant="default" disabled={busy} onClick={onAcknowledge}>
            {acknowledgeError ? 'Continue' : 'I understand'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
