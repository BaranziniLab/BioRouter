import { useRef } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';
import { disclosureTitle, type DisclosureCopy } from './disclosureCopy';

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
          {copy.long.split('\n\n').map((paragraph) => (
            <p key={paragraph.slice(0, 48)} className="min-w-0 [overflow-wrap:anywhere]">
              {paragraph}
            </p>
          ))}
        </div>

        <div className="flex justify-end pt-2">
          <Button type="button" variant="default" disabled={busy} onClick={onAcknowledge}>
            I understand
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
