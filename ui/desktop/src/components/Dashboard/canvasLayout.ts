export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface WindowRect extends Rect {
  id: string;
}

function rectsOverlap(a: Rect, b: Rect): boolean {
  const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return ow > 0 && oh > 0;
}

/** Returns true if rects `a` and `b` are closer than `gap` pixels on at least
 * one overlapping axis. Two rects that are exactly `gap` apart along their
 * separating axis are NOT considered to violate. */
function violatesGap(a: Rect, b: Rect, gap: number): boolean {
  const apart =
    a.x + a.w + gap <= b.x ||
    b.x + b.w + gap <= a.x ||
    a.y + a.h + gap <= b.y ||
    b.y + b.h + gap <= a.y;
  return !apart;
}

interface SpawnParams {
  center: { x: number; y: number };
  size: { w: number; h: number };
  existing: readonly Rect[];
  gap?: number;
  /** If provided, the new window will be placed adjacent to this rect
   * (right → below → left → above → corners), falling back to a spiral search
   * around the anchor's center if none fit. When omitted, the spiral starts at
   * the camera center. */
  anchor?: Rect | null;
}

export function findSpawnPosition({
  center,
  size,
  existing,
  gap = 16,
  anchor = null,
}: SpawnParams): { x: number; y: number } {
  const collides = (x: number, y: number) =>
    existing.some((r) => rectsOverlap({ x, y, w: size.w, h: size.h }, r));

  // Phase 1: adjacency to the active window.
  if (anchor) {
    const candidates = [
      // Cardinal sides (preferred — share a long edge with the anchor).
      { x: anchor.x + anchor.w + gap, y: anchor.y },
      { x: anchor.x, y: anchor.y + anchor.h + gap },
      { x: anchor.x - size.w - gap, y: anchor.y },
      { x: anchor.x, y: anchor.y - size.h - gap },
      // Corners.
      { x: anchor.x + anchor.w + gap, y: anchor.y + anchor.h + gap },
      { x: anchor.x - size.w - gap, y: anchor.y + anchor.h + gap },
      { x: anchor.x + anchor.w + gap, y: anchor.y - size.h - gap },
      { x: anchor.x - size.w - gap, y: anchor.y - size.h - gap },
    ];
    for (const { x, y } of candidates) {
      if (!collides(x, y)) return { x, y };
    }
    // Anchor neighborhood is full — spiral outward from the anchor center.
    const ax = anchor.x + anchor.w / 2 - size.w / 2;
    const ay = anchor.y + anchor.h / 2 - size.h / 2;
    const stepX = size.w + gap;
    const stepY = size.h + gap;
    for (let r = 2; r <= 20; r++) {
      for (let dy = -r; dy <= r; dy++) {
        for (let dx = -r; dx <= r; dx++) {
          if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
          const x = ax + dx * stepX;
          const y = ay + dy * stepY;
          if (!collides(x, y)) return { x, y };
        }
      }
    }
  }

  // Phase 2: no anchor (or anchor neighborhood exhausted). Spiral around the
  // camera center.
  const baseX = center.x - size.w / 2;
  const baseY = center.y - size.h / 2;
  if (!collides(baseX, baseY)) return { x: baseX, y: baseY };
  const stepX = size.w + gap;
  const stepY = size.h + gap;
  for (let r = 1; r <= 20; r++) {
    for (let dy = -r; dy <= r; dy++) {
      for (let dx = -r; dx <= r; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
        const x = baseX + dx * stepX;
        const y = baseY + dy * stepY;
        if (!collides(x, y)) return { x, y };
      }
    }
  }
  // Fallback: just past the existing bbox.
  const maxX = existing.reduce((m, r) => Math.max(m, r.x + r.w), baseX);
  return { x: maxX + gap, y: baseY };
}

/** Organize: pack windows together around an anchor. Windows attract toward
 * the anchor's center, but stop when moving further would violate the `gap`
 * margin against any other window. Sizes are preserved; only positions move.
 * Existing overlaps are resolved first by pushing windows apart along their
 * shorter overlap axis, keeping the anchor pinned. */
export function organize(
  windows: readonly WindowRect[],
  anchorId: string,
  gap = 16
): WindowRect[] {
  if (windows.length < 2) return windows.map((w) => ({ ...w }));

  const result = windows.map((w) => ({ ...w }));
  const anchorIdx = Math.max(
    0,
    result.findIndex((w) => w.id === anchorId)
  );
  const STEP_MAX = 24;
  const MAX_PASSES = 400;

  for (let pass = 0; pass < MAX_PASSES; pass++) {
    let movement = 0;

    // Phase A — resolve overlapping pairs AND gap-violations (rects that are
    // separated but closer than `gap`). Anchor stays pinned; non-anchor
    // partner absorbs the full push. Push direction is the axis that requires
    // the minimum movement to reach the gap margin.
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        const a = result[i];
        const b = result[j];
        // Per-axis "penalty": positive means the pair needs to move that much
        // along that axis to reach `gap` separation; zero or negative means
        // they're already separated by at least `gap` on that axis.
        const xPenL = a.x + a.w + gap - b.x; // push a left / b right
        const xPenR = b.x + b.w + gap - a.x; // push b left / a right
        const yPenT = a.y + a.h + gap - b.y; // push a up / b down
        const yPenB = b.y + b.h + gap - a.y; // push b up / a down
        // Already separated on at least one axis → nothing to do.
        if (xPenL <= 0 || xPenR <= 0 || yPenT <= 0 || yPenB <= 0) continue;
        // Pick the smallest penalty (minimum-move separation).
        let best: 'xL' | 'xR' | 'yT' | 'yB' = 'xL';
        let bestVal = xPenL;
        if (xPenR < bestVal) { best = 'xR'; bestVal = xPenR; }
        if (yPenT < bestVal) { best = 'yT'; bestVal = yPenT; }
        if (yPenB < bestVal) { best = 'yB'; bestVal = yPenB; }
        const aIsAnchor = i === anchorIdx;
        const bIsAnchor = j === anchorIdx;
        const aShare = aIsAnchor ? 0 : bIsAnchor ? bestVal : bestVal / 2;
        const bShare = bIsAnchor ? 0 : aIsAnchor ? bestVal : bestVal / 2;
        switch (best) {
          case 'xL': // a is left of b: push a further left, b further right
            a.x -= aShare;
            b.x += bShare;
            break;
          case 'xR': // b is left of a
            a.x += aShare;
            b.x -= bShare;
            break;
          case 'yT': // a is above b
            a.y -= aShare;
            b.y += bShare;
            break;
          case 'yB': // b is above a
            a.y += aShare;
            b.y -= bShare;
            break;
        }
        movement += bestVal;
      }
    }

    // Phase B — attract every non-anchor window toward the anchor on each
    // axis, blocked by the gap margin against any other window.
    const anchor = result[anchorIdx];
    const ax = anchor.x + anchor.w / 2;
    const ay = anchor.y + anchor.h / 2;
    for (let k = 0; k < result.length; k++) {
      if (k === anchorIdx) continue;
      const w = result[k];
      const cx = w.x + w.w / 2;
      const cy = w.y + w.h / 2;
      const dx = ax - cx;
      const dy = ay - cy;

      if (Math.abs(dx) > 0.5) {
        let stepX = Math.sign(dx) * Math.min(STEP_MAX, Math.abs(dx));
        // Bisect on the step until either (a) it fits, or (b) it shrinks
        // below 0.5 pixels. This lets the attractor settle into exact
        // gap-touching positions even when the full step would overshoot.
        let applied = 0;
        while (Math.abs(stepX) >= 0.5) {
          const tryRect: Rect = { x: w.x + stepX, y: w.y, w: w.w, h: w.h };
          let blocked = false;
          for (let oi = 0; oi < result.length; oi++) {
            if (oi === k) continue;
            if (violatesGap(tryRect, result[oi], gap)) {
              blocked = true;
              break;
            }
          }
          if (!blocked) {
            applied = stepX;
            break;
          }
          stepX *= 0.5;
        }
        if (applied !== 0) {
          w.x += applied;
          movement += Math.abs(applied);
        }
      }

      if (Math.abs(dy) > 0.5) {
        let stepY = Math.sign(dy) * Math.min(STEP_MAX, Math.abs(dy));
        let applied = 0;
        while (Math.abs(stepY) >= 0.5) {
          const tryRect: Rect = { x: w.x, y: w.y + stepY, w: w.w, h: w.h };
          let blocked = false;
          for (let oi = 0; oi < result.length; oi++) {
            if (oi === k) continue;
            if (violatesGap(tryRect, result[oi], gap)) {
              blocked = true;
              break;
            }
          }
          if (!blocked) {
            applied = stepY;
            break;
          }
          stepY *= 0.5;
        }
        if (applied !== 0) {
          w.y += applied;
          movement += Math.abs(applied);
        }
      }
    }

    if (movement < 1) break;
  }
  return result;
}
