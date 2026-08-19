// ui/desktop/src/components/knowledge/graph/useCanvasTheme.ts
import { useEffect, useRef, useState } from 'react';
import { useResolvedTheme } from '../../../contexts/ThemeContext';
import {
  CANVAS_FONT_FALLBACK,
  CANVAS_INK_FALLBACK,
  CANVAS_MONO_FALLBACK,
  resolveGraphTheme,
} from './graphStyle';
import type { GraphTheme, GraphThemeProbes } from './graphStyle';
import { GRAPH_PALETTE } from '../../../styles/graphPalette';

/**
 * How long after a theme change the canvas keeps re-reading the cascade.
 *
 * Long enough to outlast anything the motion system animates: `--dur-med` and
 * the Knowledge view's own `br-knowledge-fade-in` are both comfortably inside it.
 *
 * ⚠ **The window runs to the end; it does NOT stop when the value stops
 * changing.** "Two frames agreed" is exactly what the beginning of a transition
 * looks like — the interpolation has not moved off its start value yet — so an
 * early-out on stability re-introduces the bug for any transition that has a
 * delay or an ease-in. The cost is ~40 `getComputedStyle` reads, once per theme
 * change, on five elements.
 */
export const CANVAS_THEME_SETTLE_MS = 700;

export type CanvasTheme = GraphTheme;

/**
 * The refs a caller attaches to the four 0×0 probe spans.
 *
 * ⚠ **Every field is optional, and the hook's whole `probes` argument is too.**
 * That is a correctness requirement rather than a convenience: a probe ref's
 * `.current` is `null` on the first render of *every* caller, because refs are
 * attached after the render that creates them — so "this probe is not here yet"
 * is a state the resolver already has to handle, and `resolveGraphTheme` does
 * handle it by falling back per field. An *absent* probe is the same state as an
 * unattached one, so it must take the same path. Requiring the argument instead
 * put a bare `p.mono.current` on the mount path, which threw
 * `Cannot read properties of undefined (reading 'mono')` from inside the hook —
 * a crash in product code, surfacing as if it were a stale caller.
 */
export interface CanvasThemeProbeRefs {
  mono?: React.RefObject<HTMLElement | null>;
  danger?: React.RefObject<HTMLElement | null>;
  muted?: React.RefObject<HTMLElement | null>;
  border?: React.RefObject<HTMLElement | null>;
}

/**
 * Every structural colour the canvas needs, resolved off the live cascade.
 *
 * ⚠ **ONE hook, one observer, one state object — and the seven fields ride it
 * together deliberately.** The family already did this and the ink did not,
 * which is exactly how the ink came to be a hardcoded near-black: a second,
 * parallel piece of theme plumbing is a second thing to forget. §5.11 adds five
 * more fields, so the rule matters five times as much now. Anything else this
 * canvas needs from the cascade belongs here too.
 *
 * ⚠ **A theme change is read until it SETTLES, not once when it fires.** A
 * `MutationObserver` on `<html>`'s `class` / `data-theme` is the right trigger,
 * but the value it hands you at that instant can be the OLD one:
 * `getComputedStyle().color` during a CSS transition returns the current
 * INTERPOLATED value, and at the first frame that is where the transition
 * started. Measured in Chrome at observer time: `<html>` already `class="dark"`,
 * the root's `--text-default` already `#f2f3f4`, `body`'s colour already
 * `rgb(242, 243, 244)` — and this container's still `rgb(5, 32, 73)`, the light
 * ink, catching up ~175 ms later. Caching that one read left the canvas exactly
 * ONE toggle behind. A fresh load was always correct, which is what makes the
 * bug read as "only the live toggle is broken".
 *
 * Reading `body` instead would dodge the transition, and is the tempting wrong
 * answer: the container's ink is the ink for the container's CONTEXT, and a
 * canvas placed under a differently-inked subtree would then paint the page
 * default. Settling keeps the right source and simply waits for it.
 *
 * `mode` comes from `useResolvedTheme()`, which falls back to `light` outside a
 * provider instead of throwing like `useTheme()`.
 */
export function useCanvasTheme(
  ref: React.RefObject<HTMLElement | null>,
  probes?: CanvasThemeProbeRefs
): CanvasTheme {
  const mode = useResolvedTheme();
  const [theme, setTheme] = useState<CanvasTheme>(() => ({
    fontFamily: CANVAS_FONT_FALLBACK,
    monoFamily: CANVAS_MONO_FALLBACK,
    ink: CANVAS_INK_FALLBACK,
    ground: GRAPH_PALETTE[mode].ground,
    danger: null,
    muted: CANVAS_INK_FALLBACK,
    border: CANVAS_INK_FALLBACK,
    mode,
  }));

  // The probe refs are stable objects; keeping them in a ref means the effect
  // does not re-run when a caller re-creates the wrapper literal each render.
  const probesRef = useRef(probes);
  probesRef.current = probes;

  useEffect(() => {
    let frame = 0;

    const read = () => {
      // Optional chaining at every step, and `?? null` at the end, so a missing
      // `probes`, a missing field and an unattached ref all arrive at
      // `resolveGraphTheme` as the same thing: `null`, which is the input its
      // per-field fallback is written against.
      const p = probesRef.current ?? {};
      const snapshot: GraphThemeProbes = {
        mono: p.mono?.current ?? null,
        danger: p.danger?.current ?? null,
        muted: p.muted?.current ?? null,
        border: p.border?.current ?? null,
      };
      const next = resolveGraphTheme(ref.current, snapshot, mode);
      // Idempotent: a re-read that agrees returns `prev` and renders nothing,
      // which is what makes running the whole window free after it lands.
      setTheme((prev) =>
        prev.fontFamily === next.fontFamily &&
        prev.monoFamily === next.monoFamily &&
        prev.ink === next.ink &&
        prev.ground === next.ground &&
        prev.danger === next.danger &&
        prev.muted === next.muted &&
        prev.border === next.border &&
        prev.mode === next.mode
          ? prev
          : next
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
  }, [ref, mode]);

  return theme;
}
