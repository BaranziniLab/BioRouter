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

/** Organize: tight greedy shelf-pack that respects the user's relative
 * positioning of windows.
 *
 * Anchor stays exactly where it is in world space (so the camera, when it
 * re-centers on the anchor, lands the active window at viewport center).
 * Every other window snaps to the closest valid slot adjacent to an
 * already-placed window — where "closest" is measured against the window's
 * PRE-organize center, not the anchor's center. That preserves user intent:
 * if you dragged window A to the left of B before clicking Organize, A
 * will be packed into a slot on the left side and B on the right, instead
 * of both being clustered by some arbitrary canonical order. Adjacent
 * slots include cardinal edges (top/bottom/left/right of a placed window,
 * with three vertical/horizontal alignments each) and the four diagonals,
 * so any spatial arrangement reachable by gap-aligned snapping is on the
 * candidate list. Windows are processed closest-to-anchor first so they
 * grab the prime adjacent slots before more distant ones are placed. */
export function organize(windows: readonly WindowRect[], anchorId: string, gap = 16): WindowRect[] {
  if (windows.length < 2) return windows.map((w) => ({ ...w }));

  const result = windows.map((w) => ({ ...w }));
  const anchorIdx = Math.max(
    0,
    result.findIndex((w) => w.id === anchorId)
  );
  const anchor = result[anchorIdx];

  // Snapshot pre-organize centers — used both to order placement
  // (closer-to-anchor first) and to score candidate slots so each window
  // lands as close as possible to its current spot.
  const preCx = new Map<string, number>();
  const preCy = new Map<string, number>();
  for (const w of result) {
    preCx.set(w.id, w.x + w.w / 2);
    preCy.set(w.id, w.y + w.h / 2);
  }
  const ax = anchor.x + anchor.w / 2;
  const ay = anchor.y + anchor.h / 2;

  const overlaps = (a: Rect, b: Rect): boolean => {
    const ow = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const oh = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    return ow > 0 && oh > 0;
  };

  const placed: WindowRect[] = [anchor];
  const fits = (x: number, y: number, w: number, h: number): boolean => {
    const rect = { x, y, w, h };
    return !placed.some((p) => overlaps(rect, p));
  };

  // A slot "touches" a placed rect when one axis is at exactly `gap`
  // separation and the other axis overlaps — i.e. they share a full
  // gap-aligned edge. Counting touches across all placed windows lets us
  // prefer slots that snap into concave corners (touching 2 or more
  // neighbors) over slots that hang off the cluster's outer edge — which
  // is what makes a 2x2 input stay 2x2 instead of degrading to an L-shape.
  const touches = (slot: Rect, p: Rect): boolean => {
    const eps = 0.5;
    const xOverlap = Math.min(slot.x + slot.w, p.x + p.w) - Math.max(slot.x, p.x);
    const yOverlap = Math.min(slot.y + slot.h, p.y + p.h) - Math.max(slot.y, p.y);
    if (xOverlap > 0) {
      if (Math.abs(slot.y - (p.y + p.h) - gap) < eps) return true;
      if (Math.abs(p.y - (slot.y + slot.h) - gap) < eps) return true;
    }
    if (yOverlap > 0) {
      if (Math.abs(slot.x - (p.x + p.w) - gap) < eps) return true;
      if (Math.abs(p.x - (slot.x + slot.w) - gap) < eps) return true;
    }
    return false;
  };

  // Order: closer to anchor first. Ties resolved by original index (stable).
  const ordered = result
    .map((w, i) => ({ w, i }))
    .filter(({ i }) => i !== anchorIdx)
    .map(({ w, i }) => ({
      w,
      i,
      d: Math.hypot((preCx.get(w.id) ?? 0) - ax, (preCy.get(w.id) ?? 0) - ay),
    }))
    .sort((a, b) => a.d - b.d || a.i - b.i);

  for (const { w } of ordered) {
    const origCx = preCx.get(w.id) ?? 0;
    const origCy = preCy.get(w.id) ?? 0;
    interface Candidate {
      x: number;
      y: number;
      dist: number;
      shared: number;
    }
    const candidates: Candidate[] = [];
    for (const p of placed) {
      // 12 cardinal alignments (4 sides × 3 alignments) + 4 diagonals = 16
      // candidate slots per placed window. Every slot leaves `gap` between
      // the new window and the placed one along the separating axis.
      const tries: Array<{ x: number; y: number }> = [
        // Right of p — top-aligned, bottom-aligned, center-aligned.
        { x: p.x + p.w + gap, y: p.y },
        { x: p.x + p.w + gap, y: p.y + p.h - w.h },
        { x: p.x + p.w + gap, y: p.y + p.h / 2 - w.h / 2 },
        // Left of p.
        { x: p.x - w.w - gap, y: p.y },
        { x: p.x - w.w - gap, y: p.y + p.h - w.h },
        { x: p.x - w.w - gap, y: p.y + p.h / 2 - w.h / 2 },
        // Below p — left-aligned, right-aligned, center-aligned.
        { x: p.x, y: p.y + p.h + gap },
        { x: p.x + p.w - w.w, y: p.y + p.h + gap },
        { x: p.x + p.w / 2 - w.w / 2, y: p.y + p.h + gap },
        // Above p.
        { x: p.x, y: p.y - w.h - gap },
        { x: p.x + p.w - w.w, y: p.y - w.h - gap },
        { x: p.x + p.w / 2 - w.w / 2, y: p.y - w.h - gap },
        // Diagonal corners.
        { x: p.x + p.w + gap, y: p.y + p.h + gap },
        { x: p.x - w.w - gap, y: p.y + p.h + gap },
        { x: p.x + p.w + gap, y: p.y - w.h - gap },
        { x: p.x - w.w - gap, y: p.y - w.h - gap },
      ];
      for (const t of tries) {
        if (!fits(t.x, t.y, w.w, w.h)) continue;
        const cx = t.x + w.w / 2;
        const cy = t.y + w.h / 2;
        const dist = Math.hypot(cx - origCx, cy - origCy);
        const slot: Rect = { x: t.x, y: t.y, w: w.w, h: w.h };
        const shared = placed.reduce((n, q) => n + (touches(slot, q) ? 1 : 0), 0);
        candidates.push({ x: t.x, y: t.y, dist, shared });
      }
    }
    if (candidates.length === 0) {
      // Adjacent slots are all blocked — spiral outward from the window's
      // own pre-organize center to find any non-overlapping spot. This is
      // an extreme fallback; with cardinal+diagonal coverage above it
      // basically never fires for plausible window counts.
      const stepX = w.w + gap;
      const stepY = w.h + gap;
      const baseX = origCx - w.w / 2;
      const baseY = origCy - w.h / 2;
      let chosen: { x: number; y: number } | null = null;
      if (fits(baseX, baseY, w.w, w.h)) {
        chosen = { x: baseX, y: baseY };
      } else {
        spiral: for (let r = 1; r <= 30; r++) {
          for (let dy = -r; dy <= r; dy++) {
            for (let dx = -r; dx <= r; dx++) {
              if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
              const x = baseX + dx * stepX;
              const y = baseY + dy * stepY;
              if (fits(x, y, w.w, w.h)) {
                chosen = { x, y };
                break spiral;
              }
            }
          }
        }
      }
      if (chosen) {
        w.x = chosen.x;
        w.y = chosen.y;
      } else {
        // Truly nowhere — drop it to the right of the bounding box.
        const maxX = placed.reduce((m, p) => Math.max(m, p.x + p.w), anchor.x);
        w.x = maxX + gap;
        w.y = ay - w.h / 2;
      }
    } else {
      // Combined score = dist − shared × (max(w,h) + gap). The bonus per
      // shared edge is one window-step; that's small enough that a window
      // won't be pulled across the cluster to grab an extra neighbor, but
      // large enough to flip an "L extension" candidate (shared 1, close
      // to original) over a "concave corner" candidate (shared 2, one
      // window-step further) — turning L-shapes into proper 2x2 grids
      // when the user's positions are consistent with one.
      const sharedBonus = Math.max(w.w, w.h) + gap;
      const score = (c: { dist: number; shared: number }) => c.dist - c.shared * sharedBonus;
      candidates.sort((a, b) => score(a) - score(b));
      w.x = candidates[0].x;
      w.y = candidates[0].y;
    }
    placed.push(w);
  }
  return result;
}

/** @deprecated Internal helper retained for tests that exercise the
 * force-directed fallback. Real organize uses grid packing. */
export function organizeForceDirected(
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
        if (xPenR < bestVal) {
          best = 'xR';
          bestVal = xPenR;
        }
        if (yPenT < bestVal) {
          best = 'yT';
          bestVal = yPenT;
        }
        if (yPenB < bestVal) {
          best = 'yB';
          bestVal = yPenB;
        }
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
