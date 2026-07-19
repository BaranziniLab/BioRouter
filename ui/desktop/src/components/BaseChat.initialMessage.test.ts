import { describe, expect, it, vi } from 'vitest';
import { runInitialMessageAutoSubmit } from './BaseChat';

/**
 * REGRESSION GATE (BaseChat half) — duplicate submission on tab close.
 *
 * A chat tab carries `pendingInitialMessage`: the message that created its
 * session. `ChatGroupsShell` hands it to `BaseChat` as `initialMessage` and
 * `BaseChat` auto-submits it on mount. Only a group's ACTIVE tab is mounted, so
 * closing a tab REMOUNTS its successor's `BaseChat` — and the only guards were a
 * component-local ref (dies with the unmount) and a `navigate()` that clears
 * router state but not the tab record. Measured live on 2026-07-18: one message
 * plus three open/close cycles produced four real LLM turns, 23,584 input tokens.
 *
 * The fix is `onConsumed` — the owner's chance to drop the cargo. These pin
 * WHEN it fires, which is the whole subtlety: on the submitting branch only, and
 * only after the submit. Fire it on the `!hasSession` bail and the legitimate
 * FIRST submission is silently dropped instead.
 *
 * The companion test chatGroups/duplicateSubmissionWiring.test.tsx pins the
 * other half: that ChatGroupsShell actually passes a callback that clears it.
 */

/** Every collaborator, recorded, so ordering can be asserted and not just counts. */
function harness() {
  const calls: string[] = [];
  return {
    calls,
    submit: vi.fn((text: string) => {
      calls.push(`submit:${text}`);
    }),
    clearRouterState: vi.fn(() => {
      calls.push('clearRouterState');
    }),
    onConsumed: vi.fn(() => {
      calls.push('onConsumed');
    }),
  };
}

describe('runInitialMessageAutoSubmit', () => {
  it('submits the initial message and reports the cargo spent, in that order', () => {
    const h = harness();

    const submitted = runInitialMessageAutoSubmit({
      hasSession: true,
      hasAutoSubmitted: false,
      initialMessage: 'WWMARKER name one animal',
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    });

    expect(submitted).toBe(true);
    expect(h.submit).toHaveBeenCalledTimes(1);
    expect(h.submit).toHaveBeenCalledWith('WWMARKER name one animal', undefined);
    expect(h.onConsumed).toHaveBeenCalledTimes(1);
    // The cargo is dropped AFTER it is spent, never before.
    expect(h.calls).toEqual(['submit:WWMARKER name one animal', 'clearRouterState', 'onConsumed']);
  });

  it('forwards attachments alongside the message', () => {
    const h = harness();
    const attachments = [{ type: 'file', path: '/tmp/cohort.csv' }] as never;

    runInitialMessageAutoSubmit({
      hasSession: true,
      hasAutoSubmitted: false,
      initialMessage: 'summarize this',
      initialAttachments: attachments,
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    });

    expect(h.submit).toHaveBeenCalledWith('summarize this', attachments);
  });

  /**
   * THE DESIGN CONSTRAINT. A mount whose session has not resolved yet bails and
   * runs again once it has. If the owner cleared the cargo eagerly on render —
   * the obvious "simplification" — that first, legitimate message would never be
   * sent at all. So nothing may be consumed on this path.
   */
  it('does NOT consume the cargo when the session has not loaded yet', () => {
    const h = harness();

    const submitted = runInitialMessageAutoSubmit({
      hasSession: false,
      hasAutoSubmitted: false,
      initialMessage: 'WWMARKER name one animal',
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    });

    expect(submitted).toBe(false);
    expect(h.submit).not.toHaveBeenCalled();
    expect(h.onConsumed).not.toHaveBeenCalled();
  });

  it('a bailed mount still submits once the session arrives', () => {
    const h = harness();
    const args = {
      initialMessage: 'WWMARKER name one animal',
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    };

    // Effect run 1: session still resolving.
    let hasAutoSubmitted = runInitialMessageAutoSubmit({
      ...args,
      hasSession: false,
      hasAutoSubmitted: false,
    });
    // Effect run 2: session resolved.
    hasAutoSubmitted = runInitialMessageAutoSubmit({ ...args, hasSession: true, hasAutoSubmitted });

    expect(hasAutoSubmitted).toBe(true);
    expect(h.submit).toHaveBeenCalledTimes(1);
    expect(h.onConsumed).toHaveBeenCalledTimes(1);
  });

  it('never submits twice within one mount, however often the effect re-runs', () => {
    const h = harness();
    const args = {
      hasSession: true,
      initialMessage: 'WWMARKER name one animal',
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    };

    let hasAutoSubmitted = runInitialMessageAutoSubmit({ ...args, hasAutoSubmitted: false });
    hasAutoSubmitted = runInitialMessageAutoSubmit({ ...args, hasAutoSubmitted });
    hasAutoSubmitted = runInitialMessageAutoSubmit({ ...args, hasAutoSubmitted });

    expect(hasAutoSubmitted).toBe(true);
    expect(h.submit).toHaveBeenCalledTimes(1);
    expect(h.onConsumed).toHaveBeenCalledTimes(1);
  });

  it('works without an owner callback (surfaces with no tab record)', () => {
    const h = harness();

    expect(() =>
      runInitialMessageAutoSubmit({
        hasSession: true,
        hasAutoSubmitted: false,
        initialMessage: 'hello',
        shouldStartAgent: false,
        submit: h.submit,
        clearRouterState: h.clearRouterState,
        // no onConsumed — Dashboard / Hub mount BaseChat with no tab behind it
      })
    ).not.toThrow();
    expect(h.submit).toHaveBeenCalledTimes(1);
  });

  it('the shouldStartAgent path starts the agent but consumes no cargo', () => {
    const h = harness();

    const submitted = runInitialMessageAutoSubmit({
      hasSession: true,
      hasAutoSubmitted: false,
      initialMessage: undefined,
      shouldStartAgent: true,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    });

    expect(submitted).toBe(true);
    expect(h.submit).toHaveBeenCalledWith('');
    // There was no message to spend, so there is nothing to tell the owner.
    expect(h.onConsumed).not.toHaveBeenCalled();
    expect(h.clearRouterState).not.toHaveBeenCalled();
  });

  it('an idle mount (no message, no agent flag) does nothing at all', () => {
    const h = harness();

    const submitted = runInitialMessageAutoSubmit({
      hasSession: true,
      hasAutoSubmitted: false,
      shouldStartAgent: false,
      submit: h.submit,
      clearRouterState: h.clearRouterState,
      onConsumed: h.onConsumed,
    });

    expect(submitted).toBe(false);
    expect(h.calls).toEqual([]);
  });
});
