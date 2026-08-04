import { useCallback, useEffect, useState } from 'react';
import { Button } from '../ui/button';
import {
  acceptCrossAffiliationFlow,
  GRANT_SCOPE_COPY,
  readMixingMode,
  type MixingMode,
} from '../../utils/crossAffiliation';

export interface CrossAffiliationAcceptCardProps {
  /**
   * The chat the refusal happened in. A grant is keyed on it, so there is
   * nothing to record without one and the card renders nothing.
   */
  sessionId?: string;
  /** The extension key, read out of the refusal's accept frame. */
  extension: string;
}

/**
 * The user's accept control for one stated cross-institutional data flow
 * (issue #56, DR-26 / Task 57).
 *
 * ⚠ **It renders where the refusal lands** — inside the failed tool call in the
 * transcript, immediately under the daemon's own words. That placement is the
 * whole fix: the mechanism has existed since Task 49, and what was missing was
 * an affordance on the surface where the person meets the refusal. A control
 * somewhere else (a settings screen, a separate dialog they must go find) is the
 * same hard block with an extra step.
 *
 * ⚠ **The agent cannot press it.** DR-19: the model may ask, only the user may
 * answer. Three things hold that, and none of them is this component's good
 * intentions — the daemon requires `X-User-Action`, which no tool-call path can
 * carry; the grant call has exactly one caller in the renderer, audited by
 * `utils/crossAffiliation.test.ts`; and that caller is an `onClick`.
 *
 * ⚠ **What is deliberately NOT here: the risk statement.** The refusal directly
 * above this card already carries the warning verbatim, naming both
 * institutions, composed once in `privacy::affiliation` so every surface states
 * it in the same words. Restating it here would be a second account of one
 * boundary. What this adds is the half the refusal cannot state — how far a yes
 * reaches — and that comes from {@link GRANT_SCOPE_COPY}, mirrored from the
 * daemon rather than written again.
 */
export function CrossAffiliationAcceptCard({
  sessionId,
  extension,
}: CrossAffiliationAcceptCardProps) {
  // `null` while unknown. Rendering the control before the policy is read would
  // flash an accept button on a machine whose policy is `open` or `strict`, and
  // a control that appears and then withdraws is worse than one that arrives a
  // beat late.
  const [mode, setMode] = useState<MixingMode | null>(null);
  const [busy, setBusy] = useState(false);
  // ⚠ Component-local, so a remount (scrolling a long transcript, reopening the
  // chat) shows the button again for a flow that is already granted. Harmless —
  // `grant::record` is idempotent by triple and refreshes the timestamp rather
  // than erroring — but it is not the same as knowing. There is no route that
  // answers "is this triple granted", and inventing one to dim a button would
  // add a second reader of the grant store beside the gate that decides.
  const [accepted, setAccepted] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId) return;
    let live = true;
    void readMixingMode().then((next) => {
      if (live) setMode(next);
    });
    return () => {
      live = false;
    };
  }, [sessionId]);

  const approve = useCallback(async () => {
    if (!sessionId) return;
    setBusy(true);
    setError(null);
    try {
      setAccepted(await acceptCrossAffiliationFlow(sessionId, extension));
    } catch (e) {
      // Under `throwOnError` the generated client throws the PARSED BODY, so a
      // refusal arrives as a plain string and says more than any fallback could.
      // The realistic one is a daemon the app did not start, holding no
      // user-action key (open question 23).
      setError(e instanceof Error ? e.message : typeof e === 'string' && e.trim() ? e : null);
    } finally {
      setBusy(false);
    }
  }, [sessionId, extension]);

  // A grant is keyed on (session, extension, model affiliation). No session, no
  // key, nothing to offer.
  if (!sessionId) return null;

  if (accepted) {
    return (
      <div
        data-testid="cross-affiliation-accepted"
        role="status"
        className="mt-3 rounded-lg border border-border-subtle px-3 py-3 text-sm text-text-default"
      >
        {/*
          The daemon's own composition (`grant::accepted_statement`), echoed
          back and printed verbatim. The sentence recorded and the sentence read
          must not differ by a word — a paraphrase here would leave the audit
          row saying one thing and the user remembering another.
        */}
        <p className="min-w-0 [overflow-wrap:anywhere]">{accepted}</p>
        {/*
          ⚠ The one thing the daemon's sentence cannot say: what happens next.
          The refusal told the model *do not retry — the same call will be
          refused again*, and it was right when it said it, so the model will
          not try on its own. Without this line the user is left holding a
          recorded acceptance and a conversation that has stopped, which is the
          same dead end one press further along.
        */}
        <p className="min-w-0 [overflow-wrap:anywhere] mt-2 text-text-muted">
          Ask the assistant to try that step again — it was told not to retry on its own.
        </p>
      </div>
    );
  }

  // DR-27's policy, read in ONE place. `open` raises no mismatch in the daemon,
  // so in practice no refusal reaches this component at all; the branch is here
  // because a control must not depend on an upstream gate staying silent.
  if (mode === null || mode === 'open') return null;

  if (mode === 'strict') {
    return (
      <div
        data-testid="cross-affiliation-strict"
        className="mt-3 rounded-lg border border-border-subtle px-3 py-3 text-sm text-text-default space-y-2"
      >
        {/*
          ⚠ **Fails closed, and that is not the same as unfinished.** Strict
          requires DR-20's system password on top of the in-app proof, and no
          surface in this build can raise one (its prompter has no callers yet).
          Accepting on the weaker proof would be the control quietly downgrading
          itself under exactly the policy that asked for more, so it is not
          offered. The other way out — a model covered by the same institution's
          agreements — is unchanged and is named.
        */}
        <p className="min-w-0 [overflow-wrap:anywhere]">
          This machine&rsquo;s privacy policy is set to <strong>strict</strong>, which requires a
          system password before a cross-institutional flow can be accepted. This build cannot ask
          for one here, so the flow cannot be approved from this chat.
        </p>
        <p className="min-w-0 [overflow-wrap:anywhere] text-text-muted">
          Switch this chat to a model covered by the same institution&rsquo;s agreements, or ask
          whoever set the policy to relax it.
        </p>
      </div>
    );
  }

  return (
    <div
      data-testid="cross-affiliation-accept"
      className="mt-3 rounded-lg border border-border-subtle px-3 py-3 text-sm text-text-default space-y-3"
    >
      {/*
        The scope, BEFORE the press. "How far does my yes reach" is part of what
        is being decided: a user who believes they are approving one call behaves
        differently from one who knows they are approving this connector for the
        rest of the chat. It is also what makes the narrowness legible.
      */}
      <p className="min-w-0 [overflow-wrap:anywhere]">{GRANT_SCOPE_COPY}</p>
      {error && (
        <p role="alert" className="min-w-0 [overflow-wrap:anywhere] text-text-muted">
          Nothing was recorded. {error}
        </p>
      )}
      <Button
        type="button"
        variant="default"
        size="sm"
        disabled={busy}
        onClick={() => void approve()}
      >
        {/*
          Names the connector, so a transcript with two refused calls in it does
          not present two identical buttons.
        */}
        Approve this flow for {extension}
      </Button>
    </div>
  );
}
