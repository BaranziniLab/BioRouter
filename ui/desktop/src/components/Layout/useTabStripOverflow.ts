import { RefObject, useCallback, useEffect, useLayoutEffect, useState } from 'react';
import { shouldShowTabOverflowMenu } from './yieldLadder';

/**
 * Rung 3 of the yield ladder (D-32), as one rule for every tab strip.
 *
 * "Tabs shrink to a floor, then scroll, then collapse into a ▾ menu — never
 * wrap." The shrink and the scroll are CSS (`.br-tabstrip`); this is the last
 * step — whether the strip currently hides a tab, and therefore needs the menu.
 *
 * It lives here, and not inside ChatTabStrip, because the spec's card Z is
 * explicit that the chat strip and the preview panel are the SAME tab language.
 * They are still two components today, so a rule implemented in one of them is a
 * rule the other silently lacks — which is exactly what happened: the panel's
 * strip was `overflow-hidden`, so its tabs did not even scroll. They were simply
 * clipped and unreachable.
 *
 * @param ref the element with `overflow-x: auto` that holds the tabs
 * @param tabCount how many tabs it holds
 */
export function useTabStripOverflow(ref: RefObject<HTMLElement | null>, tabCount: number): boolean {
  const [overflowing, setOverflowing] = useState(false);

  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setOverflowing(
      shouldShowTabOverflowMenu({
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
        tabCount,
      })
    );
  }, [ref, tabCount]);

  // Two triggers, because they are two different events and neither implies the
  // other: the OBSERVER catches the strip's box changing (window resize, sidebar
  // collapse, splitter drag, the preview panel taking its column), and the layout
  // effect catches the CONTENT changing (a tab opened, closed or renamed) — which
  // moves scrollWidth while leaving the observed box identical, so the observer
  // never fires for it.
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

  return overflowing;
}
