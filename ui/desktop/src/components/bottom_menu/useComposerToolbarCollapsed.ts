import { RefObject, useCallback, useEffect, useLayoutEffect, useState } from 'react';
import { shouldCollapseComposerToolbar } from '../Layout/yieldLadder';

/**
 * Rung 3b of the yield ladder (D-32), for the composer's bottom control row.
 *
 * "The pickers shrink to their floor, then collapse behind a single `+` — never
 * overlap." The shrinking (working-dir path, model name) is CSS; this is the
 * last step: whether the row is now too narrow to lay every control out, and so
 * must show the `+` menu instead.
 *
 * Mirrors {@link useTabStripOverflow} exactly, including its two triggers and
 * its no-latch reasoning: the measured element is the toolbar's OWN box, whose
 * width the pane fixes independently of whether the controls are collapsed, so
 * showing/hiding the `+` cannot change the measurement and the two states can
 * never chase each other.
 *
 * @param ref the toolbar container element whose width gates the collapse
 */
export function useComposerToolbarCollapsed(ref: RefObject<HTMLElement | null>): boolean {
  const [collapsed, setCollapsed] = useState(false);

  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setCollapsed(shouldCollapseComposerToolbar({ availableWidth: el.clientWidth }));
  }, [ref]);

  // The OBSERVER catches the box changing (window resize, sidebar collapse,
  // splitter drag, the artifact panel taking its column); the layout effect
  // catches a first mount / a content swap that leaves the box identical.
  useLayoutEffect(() => {
    measure();
  });

  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref, measure]);

  return collapsed;
}
