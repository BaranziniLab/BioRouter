// ui/desktop/src/components/knowledge/graph/graphPreferences.ts
import { useCallback, useEffect, useState } from 'react';

/**
 * The canvas's user preferences (redesign R-04).
 *
 * ⚠ **`shapeChannel` is an accessibility escape hatch, not a cosmetic toggle,
 * and that is why it exists at all.** The canvas used to draw seven silhouettes
 * — shape carried the node FAMILY, fill lightness carried the member — and that
 * split was the redundant, monochrome-safe channel WCAG 1.4.1 asks for. It was
 * not chosen by taste: cross-family colour distance under simulated dichromacy
 * bottoms out at ΔE00 **0.00** (dark tritanopia, `Phenotype`/`Food`), so hue
 * cannot separate 28 marks on its own and never could.
 *
 * The redesign draws every node as a circle by default because seven
 * silhouettes at 6–12px read as noise rather than as structure, and because the
 * remaining channels — a solid/hollow split on the top-level 20-vs-8 division,
 * always-on haloed labels, an interactive legend and the inspector — carry the
 * identification work for most users. **That is a deliberate trade against a
 * measured accessibility property, not a free win**, and this preference is the
 * half that makes it reversible. Deleting it turns a recorded trade into a
 * silent regression.
 *
 * ⚠ **Default OFF is a product decision the spec records; do not flip it here.**
 * If it is ever flipped, `docs/knowledge-base/knowledge-ui-redesign/redesign-spec.md`
 * R-04 is the record that has to move with it.
 */
const SHAPE_CHANNEL_KEY = 'biorouter:knowledge-shape-channel';

/**
 * Read the stored preference.
 *
 * In a `try`/`catch` because `localStorage` throws outright in a sandboxed
 * frame, and a canvas that cannot remember a preference is far better than one
 * that takes the whole panel down with it — the same reason `GraphLegend` wraps
 * its own read.
 */
export function readShapeChannel(): boolean {
  try {
    return window.localStorage.getItem(SHAPE_CHANNEL_KEY) === 'true';
  } catch {
    return false;
  }
}

export function writeShapeChannel(on: boolean): void {
  try {
    window.localStorage.setItem(SHAPE_CHANNEL_KEY, String(on));
  } catch {
    /* a preference that cannot persist still applies for this session */
  }
}

/**
 * `[enabled, setEnabled]` for the shape channel.
 *
 * The `storage` listener is not decoration: the preference is per-machine
 * rather than per-window, and Knowledge can be open in two windows at once
 * (`newWindowMenu`). Without it, turning shapes on in one window would leave
 * the other drawing circles while its own legend drew silhouettes — the exact
 * legend-disagrees-with-canvas failure `nodeMark.ts` exists to prevent.
 */
export function useShapeChannel(): [boolean, (on: boolean) => void] {
  const [enabled, setEnabled] = useState(readShapeChannel);

  useEffect(() => {
    function onStorage(e: StorageEvent) {
      if (e.key === SHAPE_CHANNEL_KEY) setEnabled(readShapeChannel());
    }
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  const set = useCallback((on: boolean) => {
    writeShapeChannel(on);
    setEnabled(on);
  }, []);

  return [enabled, set];
}
