import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';
import { refreshSessionList } from '../../utils/sessionListCache';
import type { Session } from '../../api';

/**
 * The numbers the day-one notice quotes, over the population **History actually
 * shows** (issue #56 §15.5).
 *
 * The mirror of `biorouter::session::session_manager::PrivacyNoticeCounts`, and
 * the two agree by construction rather than by coincidence: `GET /sessions`
 * without `include_subagents` is backed by `list_sessions_by_types(&[User,
 * Scheduled])`, whose `INNER JOIN messages` selects exactly the rows the Rust
 * helper's `EXISTS (SELECT 1 FROM messages …)` selects. Counting the array the
 * renderer already holds is therefore the same question asked of the same rows,
 * with no second endpoint to keep in step.
 */
export interface NoticeCounts {
  /** Private to the fail-closed reader, and visible in History. */
  privateVisible: number;
  /** Public, visible, and bound to a named provider — read, not guessed. */
  publicNamedVisible: number;
  /**
   * Public, visible, and bound to NO provider. The migration could not tell what
   * these ran on and failed **open**, by decision. Historically the largest of
   * the three, and the honest residual of the whole feature.
   */
  unknownProviderVisible: number;
  /** The denominator: everything History will list. */
  totalVisible: number;
  /**
   * The private rows grouped by the provider the migration read, descending by
   * count. This is what makes §15.5's "review by provider" list possible: a
   * backfill tier is a **guess the system made from the last-used provider**,
   * not a user's assertion about content, so the notice owes the user a way to
   * see the guess broken down before living with it.
   */
  privateByProvider: Array<{ provider: string; count: number }>;
}

/** A `privacy_reason` of `backfill:<provider>` yields `<provider>`. */
const BACKFILL_PREFIX = 'backfill:';

/** The bucket a private row with no readable provenance lands in. */
export const UNKNOWN_PROVIDER = 'unknown';

/**
 * Fail-closed, exactly like `SessionClassification::from_stored` and like
 * `PrivacyBadge`'s `Record` map: anything that is not precisely `'public'` reads
 * Private.
 *
 * That includes `undefined` — a daemon too old to send the column, or a
 * projection that dropped it. The alternative (treat unknown as public) would
 * make the notice quote a smaller, friendlier number than the badges beside it,
 * and the one property the notice must have is that it describes what the user
 * is about to see.
 */
function isPrivate(session: Session): boolean {
  return session.privacy_tier !== 'public';
}

function providerOf(session: Session): string {
  // The provenance first, because it records what the migration actually
  // concluded and survives a later provider switch; `provider_name` is the
  // fallback for a row raised by a turn rather than by the backfill.
  const reason = session.privacy_reason ?? '';
  if (reason.startsWith(BACKFILL_PREFIX)) {
    const named = reason.slice(BACKFILL_PREFIX.length).trim();
    if (named !== '') return named;
  }
  const bound = (session.provider_name ?? '').trim();
  return bound === '' ? UNKNOWN_PROVIDER : bound;
}

/**
 * Count the user's own conversations. Pure, and the only place the rule lives.
 *
 * ⚠ **Never hardcode these numbers.** §16's table was re-measured twice while
 * the feature was being designed and moved by a factor of three in four days;
 * the `user` NULL-provider bucket alone went from 29 rows to 2,831. A notice
 * carrying a figure from a design document is a notice that is wrong on every
 * machine including the author's.
 */
export function computeNoticeCounts(sessions: Session[]): NoticeCounts {
  let privateVisible = 0;
  let publicNamedVisible = 0;
  let unknownProviderVisible = 0;
  const byProvider = new Map<string, number>();

  for (const session of sessions) {
    if (isPrivate(session)) {
      privateVisible += 1;
      const provider = providerOf(session);
      byProvider.set(provider, (byProvider.get(provider) ?? 0) + 1);
      continue;
    }
    if ((session.provider_name ?? '').trim() === '') {
      unknownProviderVisible += 1;
    } else {
      publicNamedVisible += 1;
    }
  }

  const privateByProvider = [...byProvider.entries()]
    .map(([provider, count]) => ({ provider, count }))
    // Count descending, then name ascending — a stable order, so the list does
    // not reshuffle between two renders of the same data.
    .sort((a, b) => b.count - a.count || a.provider.localeCompare(b.provider));

  return {
    privateVisible,
    publicNamedVisible,
    unknownProviderVisible,
    totalVisible: sessions.length,
    privateByProvider,
  };
}

/**
 * How a person names each private-tier provider. Unknown ids pass through
 * unchanged rather than being dropped: a provider this build has never heard of
 * still marked the user's chats, and hiding it would leave a row in the review
 * list the user cannot account for.
 */
const PROVIDER_LABELS: Record<string, string> = {
  versa_azure: 'Versa (Azure)',
  versa_bedrock: 'Versa (Bedrock)',
  llamacpp: 'Llama Server',
  ollama: 'Ollama',
  [UNKNOWN_PROVIDER]: 'Provider not recorded',
};

export function providerLabel(provider: string): string {
  return PROVIDER_LABELS[provider] ?? provider;
}

/**
 * Worth showing at all? Nothing changed for this user if no visible chat came
 * out of the backfill private.
 *
 * Exported so the surface that mounts the notice can decide **before** rendering
 * a modal, rather than opening one that says "0 conversations changed".
 */
export function shouldShowFirstRunNotice(counts: NoticeCounts): boolean {
  return counts.privateVisible > 0;
}

export interface FirstRunPrivacyNoticeProps {
  open: boolean;
  /** Acknowledged. The caller owns remembering that it was. */
  onDismiss: () => void;
  /**
   * Counts to render instead of fetching. For a caller that already holds the
   * session list (and for tests). When absent the notice reads the user's own
   * database through the same cache History does.
   */
  counts?: NoticeCounts;
}

/**
 * The day-one notice (issue #56 §15.5).
 *
 * ⚠ **It exists because the alternative is a week of unexplained refusals.** The
 * backfill marks a large fraction of an established user's history private in
 * one step — on the machine this was measured against, 963 of the 1,551 user
 * conversations whose provider was known. Every one of those chats then refuses
 * the commercial model its owner normally reaches for, with a modal that
 * explains the rule but not why *this* chat is subject to it. One screen of
 * "here is what just changed, and here is the number" is the whole mitigation.
 *
 * ⚠ **Three honesties it is required to carry**, each of which the design would
 * otherwise leave for the user to discover:
 *
 * 1. **The counts are computed, never quoted.** See {@link computeNoticeCounts}.
 * 2. **The tier is a guess from the LAST provider, not a reading of the
 *    transcript.** A chat that ran on Versa and was later switched to a
 *    commercial model backfills *public* even though its transcript holds
 *    private-model work. There is no content scan and there will not be one, so
 *    the notice says this in one sentence and tells the user the one repair:
 *    switch it back to a private model and the next turn marks it.
 * 3. **The review list is broken down by provider**, because a `backfill:*` tier
 *    is the system's inference and not the user's assertion — unlike a `turn:*`
 *    or `mcp:*` tier, which records something that actually happened.
 *
 * ⚠ **Dismissible, unlike `NonPrivateModelDisclosure`.** That one gates an
 * action and has a fact the user must be shown before taking it; this one
 * reports something that has already happened to data at rest. Trapping someone
 * in it buys nothing, and a modal that cannot be closed is the surest way to
 * make the next one go unread.
 */
export function FirstRunPrivacyNotice({ open, onDismiss, counts }: FirstRunPrivacyNoticeProps) {
  const [fetched, setFetched] = useState<NoticeCounts | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!open || counts) return;
    let cancelled = false;
    setFailed(false);
    refreshSessionList(false)
      .then((sessions) => {
        if (!cancelled) setFetched(computeNoticeCounts(sessions));
      })
      .catch(() => {
        // Say so rather than rendering zeroes. A notice that silently reports
        // "0 conversations changed" because a fetch failed is worse than one
        // that admits it could not count: the user acts on the first.
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [open, counts]);

  const resolved = counts ?? fetched;

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onDismiss()}>
      <DialogContent data-testid="first-run-privacy-notice" className="sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle>Some of your chats are now marked private</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 text-sm text-text-default">
          {failed && (
            <p data-testid="notice-count-error">
              Biorouter could not read your chat list, so the numbers below are unavailable. Your
              chats are unchanged by this message.
            </p>
          )}

          {!failed && !resolved && <p data-testid="notice-counting">Counting your chats…</p>}

          {!failed && resolved && (
            <>
              <p data-testid="notice-headline">
                <strong>{resolved.privateVisible}</strong> of your{' '}
                <strong>{resolved.totalVisible}</strong> chats are now marked private, because that
                is the model each of them was last using.
              </p>

              {resolved.privateByProvider.length > 0 && (
                <div>
                  <p className="text-text-muted">Marked private by model:</p>
                  <ul data-testid="notice-by-provider" className="mt-1 space-y-0.5">
                    {resolved.privateByProvider.map(({ provider, count }) => (
                      <li key={provider} data-provider={provider}>
                        {providerLabel(provider)} — {count}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {resolved.unknownProviderVisible > 0 && (
                <p data-testid="notice-unknown">
                  <strong>{resolved.unknownProviderVisible}</strong> chats record no model at all.
                  Biorouter cannot tell what those ran on, so they are left public.
                </p>
              )}
            </>
          )}

          <p data-testid="notice-last-model-caveat">
            Chats from before this version are marked by the model they were last using. If an older
            chat contains work you want kept private, switch it to a private model — it will be
            marked private from its next turn on.
          </p>

          <p data-testid="notice-knowledge-bases" className="text-text-muted">
            Knowledge bases that already existed start public, whatever fed them. If one holds
            private material you can mark it private yourself from the Knowledge view.
          </p>
        </div>

        <div className="flex justify-end">
          <Button type="button" onClick={onDismiss} data-testid="notice-acknowledge">
            Got it
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
