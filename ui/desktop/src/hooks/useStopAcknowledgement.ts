import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * How long the Stop control holds its "received" state.
 *
 * Tuned against the motion scale in styles/main.css (--motion-fast 120ms /
 * --motion-base 180ms / --motion-slow 260ms): long enough that the confirmation
 * completes its own transition and is legible as deliberate, short enough that
 * it cannot outlive the stop it is confirming and start lying about a turn that
 * already ended.
 */
export const STOP_ACK_MS = 450;

export interface StopAcknowledgement {
  /** True while the confirmation is on screen. */
  acknowledged: boolean;
  /** Call INSTEAD of onStop: acknowledges first, then stops. */
  trigger: () => void;
}

/**
 * Stopping a turn is the most intrusive thing the composer can do, and it used
 * to be the least confirmed: the click ran `onStop()` and nothing about the
 * button changed. The turn's own teardown is asynchronous (the SSE abort plus a
 * `/agent/cancel` round-trip), so for a beat the UI looked identical to a
 * missed click — which is exactly when a user clicks again.
 *
 * This gives the press an immediate, self-retiring acknowledgement that is
 * owned by the button rather than by the turn, so it appears on the press and
 * not on the server's reply.
 *
 * Reduced motion needs no branch here: the visual is a CSS transition plus a
 * ring, and the global prefers-reduced-motion reset in styles/main.css (DR-13)
 * nulls animation AND transition durations, so it degrades to an instant state
 * change rather than being suppressed entirely. The acknowledgement still
 * happens — it just does not animate.
 */
export function useStopAcknowledgement(onStop?: () => void): StopAcknowledgement {
  const [acknowledged, setAcknowledged] = useState(false);
  const timerRef = useRef<number | null>(null);

  // Cancel on unmount: without this a stop that unmounts the composer (a
  // session switch, say) leaves a timer that setStates into a dead component.
  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    []
  );

  const trigger = useCallback(() => {
    setAcknowledged(true);
    // A second press restarts the window rather than inheriting the remainder
    // of the first, so an impatient double-click still ends in a full-length
    // confirmation instead of a flicker.
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setAcknowledged(false);
    }, STOP_ACK_MS);

    // Acknowledge BEFORE stopping. `onStop` synchronously flips chatState to
    // Idle, which re-renders the composer back to its Send face; setting the
    // flag first means the state is already committed in the same React batch.
    onStop?.();
  }, [onStop]);

  return { acknowledged, trigger };
}
