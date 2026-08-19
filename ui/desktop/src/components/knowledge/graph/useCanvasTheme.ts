// ui/desktop/src/components/knowledge/graph/useCanvasTheme.ts
import { useEffect, useState } from 'react';
import {
  CANVAS_FONT_FALLBACK,
  CANVAS_INK_FALLBACK,
  resolveCanvasFontFamily,
  resolveCanvasInk,
} from './graphStyle';

/**
 * How long after a theme change the canvas keeps re-reading the cascade.
 *
 * Long enough to outlast anything the motion system animates: `--motion-slow`
 * and the Knowledge view's own 300 ms `br-knowledge-fade-in` are both
 * comfortably inside it.
 *
 * ⚠ **The window runs to the end; it does NOT stop when the value stops
 * changing.** "Two frames agreed" is exactly what the beginning of a transition
 * looks like — the interpolation has not moved off its start value yet — so an
 * early-out on stability re-introduces the bug for any transition that has a
 * delay or an ease-in. The cost is ~40 `getComputedStyle` reads, once per theme
 * change, on one element.
 */
export const CANVAS_THEME_SETTLE_MS = 700;

export interface CanvasTheme {
  fontFamily: string;
  ink: string;
}

/**
 * The resolved UI font family AND label ink, read off the graph container's
 * computed style.
 *
 * Both values ride ONE observer deliberately. The family already did this and
 * the ink did not, which is exactly how the ink came to be a hardcoded
 * near-black: a second, parallel piece of theme plumbing is a second thing to
 * forget. Anything else this canvas needs from the cascade belongs here too.
 *
 * ⚠ **A theme change is read until it SETTLES, not once when it fires.** A
 * `MutationObserver` on `<html>`'s `class` / `data-theme` is the right trigger
 * and was already here, but the value it hands you at that instant can be the
 * OLD one: `getComputedStyle().color` during a CSS transition returns the
 * current interpolated value, and at the first frame that is where the
 * transition started. The Knowledge view puts this canvas inside a
 * `TabsContent` whose `animate-in fade-in duration-[var(--motion-base)]`
 * resolves to `transition-property: all; transition-duration: 0.175s` — and
 * `all` includes the inherited `color`. Measured in Chrome at observer time:
 * `<html>` already `class="dark"`, the root's `--text-default` already
 * `#f2f3f4`, `body`'s colour already `rgb(242, 243, 244)` — and this
 * container's still `rgb(5, 32, 73)`, the light ink, catching up ~175 ms later.
 * Caching that one read left the canvas exactly ONE toggle behind: labels drawn
 * in the previous mode's ink, near-invisible against the new ground, until
 * something remounted it. A fresh load was always correct, which is what makes
 * the bug read as "only the live toggle is broken".
 *
 * Reading `body` instead would dodge the transition, and is the tempting wrong
 * answer: the container's ink is the ink for the container's CONTEXT, and a
 * canvas placed under a differently-inked subtree would then paint the page
 * default. Settling keeps the right source and simply waits for it.
 */
export function useCanvasTheme(ref: React.RefObject<HTMLElement | null>): CanvasTheme {
  const [theme, setTheme] = useState<CanvasTheme>({
    fontFamily: CANVAS_FONT_FALLBACK,
    ink: CANVAS_INK_FALLBACK,
  });

  useEffect(() => {
    let frame = 0;

    const read = () => {
      const next: CanvasTheme = {
        fontFamily: resolveCanvasFontFamily(ref.current),
        ink: resolveCanvasInk(ref.current),
      };
      // Idempotent: a re-read that agrees returns `prev` and renders nothing,
      // which is what makes running the whole window free after it lands.
      setTheme((prev) =>
        prev.fontFamily === next.fontFamily && prev.ink === next.ink ? prev : next
      );
    };

    const cancel = () => {
      if (frame && typeof cancelAnimationFrame === 'function') cancelAnimationFrame(frame);
      frame = 0;
    };

    const settle = (deadline: number) => {
      read();
      if (Date.now() >= deadline || typeof requestAnimationFrame !== 'function') {
        frame = 0;
        return;
      }
      frame = requestAnimationFrame(() => settle(deadline));
    };

    const start = () => {
      cancel();
      settle(Date.now() + CANVAS_THEME_SETTLE_MS);
    };

    start();

    if (typeof MutationObserver !== 'function') return cancel;
    const observer = new MutationObserver(start);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'data-theme'],
    });
    return () => {
      observer.disconnect();
      cancel();
    };
  }, [ref]);

  return theme;
}
