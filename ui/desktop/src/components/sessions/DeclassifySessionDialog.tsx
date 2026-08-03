import { useCallback, useEffect, useState } from 'react';
import { declassifySession, type Session } from '../../api';
import { toastError, toastSuccess } from '../../toasts';
import { userActionHeaders } from '../../utils/userAction';
import { DangerousConfirmDialog } from '../ui/DangerousConfirmDialog';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';

/**
 * Issue #56 §12.4's graded confirmation, mirrored from
 * `requires_typed_confirmation` in `crates/biorouter/src/privacy/declassify.rs`.
 * The daemon re-derives the same grade from the stored provenance and refuses a
 * request that skips the phrase, so this decides which control is SHOWN, never
 * whether the phrase is enforced.
 *
 * Phrased as an exception, not a match, for the reason spelled out on the Rust
 * side: `privacy_reason`'s vocabulary is `turn:*`, `mcp:*`, `inherited:<parent>`,
 * `diverged:<parent>`, `backfill:<provider>` and `imported`, so a chat branched
 * out of an OMOP session carries `diverged:<parent>` and none of the `mcp:`
 * spelling. Reading the absence of `turn:` as "unknown, so protect it" gets the
 * inherited cases right without walking ancestors, and gets an absent or
 * unrecognised reason right too.
 */
export function requiresTypedConfirmation(privacyReason?: string | null): boolean {
  return !(privacyReason ?? '').startsWith('turn:');
}

/**
 * The last six characters of the session id — what §12.4 asks the user to
 * retype, and what `confirmation_phrase` derives on the daemon.
 *
 * Characters, not `slice(-6)` on the raw string: a surrogate pair would be cut
 * in half by the latter. NOT the session NAME — `is_default_session_name` shows
 * "New Session", "CLI Session", "Session <N>" and "New session <N>" are all live
 * placeholders, so a name-typed phrase is either a string dozens of rows share
 * (destroying the justification, which is to make the user check WHICH
 * conversation) or a whole sentence to retype.
 */
export function confirmationPhrase(sessionId: string): string {
  return [...sessionId].slice(-6).join('');
}

type Phase = 'confirm' | 'undo' | 'sending';

export interface DeclassifySessionDialogProps {
  session: Session;
  /**
   * Defaults to open. This dialog is mounted by the control that opens it
   * (History's row menu, the session page's action bar) and unmounted when it
   * closes, so there is no state to keep for a closed one.
   */
  open?: boolean;
  onClose?: () => void;
  onDeclassified?: (sessionId: string) => void;
  /** The undo window on the single-click path. A seam for tests, not a setting. */
  undoMs?: number;
}

/**
 * Issue #56 §12.4/§12.5 — the one surface that marks a private chat public.
 *
 * Shared by both entry points (History's row menu and the session page) so the
 * two cannot come to disagree about what confirmation this takes. It is
 * deliberately NOT reachable from `SessionNamePill`'s title menu: that menu is
 * rename/diverge, one careless click from the chat title, and §12.1 puts this
 * on the History row instead.
 *
 * Two controls, graded on provenance:
 *
 *  * A chat that reached a private **data source** (`mcp:*`, or anything whose
 *    provenance is not a plain turn) asks the user to retype the last six
 *    characters of its id, shown beside the field.
 *  * A chat that merely ran turns against a private **model** gets a single
 *    click, with an undo window — and the request is HELD for that window
 *    rather than sent and reversed, because declassification is one-way at the
 *    daemon: a re-raise would write a second ledger row under a different
 *    provenance, leaving an audit trail that says something happened which the
 *    user believes did not.
 */
export function DeclassifySessionDialog({
  session,
  open = true,
  onClose,
  onDeclassified,
  undoMs = 5000,
}: DeclassifySessionDialogProps) {
  const [phase, setPhase] = useState<Phase>('confirm');
  const needsPhrase = requiresTypedConfirmation(session.privacy_reason);
  const phrase = confirmationPhrase(session.id);
  const undoSeconds = Math.max(1, Math.round(undoMs / 1000));

  const send = useCallback(
    async (confirmation: string | null) => {
      setPhase('sending');
      try {
        await declassifySession({
          path: { session_id: session.id },
          body: { confirmation },
          // DR-16's proof-of-user. Without it the daemon refuses, correctly:
          // the server secret alone is reachable from any developer-enabled
          // agent shell (§9.3 A1) and is not evidence of a human.
          headers: await userActionHeaders(),
          throwOnError: true,
        });
        toastSuccess({
          title: 'Chat marked public',
          msg: 'It no longer carries a private marker. The change is recorded.',
        });
        onDeclassified?.(session.id);
        onClose?.();
      } catch (error) {
        toastError({
          title: 'Could not mark this chat public',
          msg: error instanceof Error ? error.message : String(error),
        });
        setPhase('confirm');
      }
    },
    [onClose, onDeclassified, session.id]
  );

  // The undo window. The request has NOT been made while this is pending —
  // that is the whole difference between an undo and an apology.
  useEffect(() => {
    if (phase !== 'undo') return;
    const timer = setTimeout(() => void send(null), undoMs);
    return () => clearTimeout(timer);
  }, [phase, send, undoMs]);

  if (phase === 'undo') {
    return (
      <Dialog open={open} onOpenChange={(next) => !next && setPhase('confirm')}>
        <DialogContent className="sm:max-w-[480px]">
          <DialogHeader>
            <DialogTitle>Marking this chat public…</DialogTitle>
            <DialogDescription>
              Nothing has been sent yet. This chat becomes public in {undoSeconds} seconds unless
              you undo.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="pt-2">
            <Button type="button" variant="outline" onClick={() => setPhase('confirm')}>
              Undo
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }

  return (
    <DangerousConfirmDialog
      open={open}
      busy={phase === 'sending'}
      title="Make this chat public?"
      description={
        needsPhrase
          ? 'This chat reached a private data source. Marking it public removes its private ' +
            'marker and lets it be opened by a public model — its contents are unchanged, and ' +
            'this cannot be undone.'
          : `This chat only ran turns against a private model. Marking it public removes its ` +
            `private marker; you will have ${undoSeconds} seconds to undo.`
      }
      phrase={needsPhrase ? phrase : undefined}
      fieldLabel="Type the last 6 characters of this chat's ID"
      confirmLabel="Make public"
      onConfirm={() => (needsPhrase ? void send(phrase) : setPhase('undo'))}
      onCancel={() => onClose?.()}
    >
      {/* The ResetPanel precedent: show what is being acted on, so a user who
          opened the menu on the wrong row sees it before confirming. */}
      <div className="rounded-lg border border-border-subtle bg-background-muted/40 p-3">
        <p className="truncate text-sm font-medium text-text-default" title={session.name}>
          {session.name}
        </p>
        <p className="mt-0.5 font-mono text-xs text-text-muted">{session.id}</p>
      </div>
    </DangerousConfirmDialog>
  );
}
