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

/** Pick a column count that produces a square-ish, landscape-leaning grid.
 * Tuned to match user-sketched expectations:
 *   n=1 → 1, n=2 → 2, n=3 → 3, n=4 → 2, n=5..6 → 3, n=7..9 → 3,
 *   n=10..12 → 4, n=13..16 → 4, n>16 → ceil(sqrt(n)).
 * (Single row for n ≤ 3 since that always reads more orderly than 2x2.) */
function chooseCols(n: number): number {
  if (n <= 3) return n;
  return Math.ceil(Math.sqrt(n));
}

/** Organize: arrange windows into a tidy grid centered on the anchor.
 * Sizes are preserved; only positions move. When every window has the same
 * size we produce a regular row-major grid with the partial (final) row
 * centered above the full rows for visual balance. When sizes differ we
 * fall back to shelf packing: rows are sized by their tallest member, and
 * each row is centered horizontally over the anchor. Either way the
 * anchor's world position stays put — the grid is laid out around it. */
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
  const anchor = result[anchorIdx];

  // Uniform-size path — produce a centered grid.
  const sameSize = result.every((w) => w.w === anchor.w && w.h === anchor.h);
  if (sameSize) {
    const n = result.length;
    const cols = Math.min(n, chooseCols(n));
    const rows = Math.ceil(n / cols);
    const partialCount = n - (rows - 1) * cols; // count in the first (partial) row
    const cellW = anchor.w;
    const cellH = anchor.h;
    const stepX = cellW + gap;
    const stepY = cellH + gap;
    const totalH = rows * cellH + (rows - 1) * gap;

    // Position the grid so the anchor's slot lands at the anchor's current world center.
    const ax = anchor.x + anchor.w / 2;
    const ay = anchor.y + anchor.h / 2;
    // Compute the anchor's row/col so we can shift the grid to align it with
    // the anchor's current center on BOTH axes (so the anchor doesn't move).
    let anchorRow: number;
    let anchorIndexInRow: number;
    let anchorRowCount: number;
    if (anchorIdx < partialCount) {
      anchorRow = 0;
      anchorIndexInRow = anchorIdx;
      anchorRowCount = partialCount;
    } else {
      const offset = anchorIdx - partialCount;
      anchorRow = 1 + Math.floor(offset / cols);
      anchorIndexInRow = offset % cols;
      anchorRowCount = cols;
    }
    // Y shift: anchor row's center should equal ay.
    const naiveGridOriginY = -totalH / 2;
    const naiveAnchorRowCenterY = naiveGridOriginY + anchorRow * stepY + cellH / 2;
    const gridShiftY = ay - naiveAnchorRowCenterY;
    // X shift: anchor's slot center within its row should equal ax.
    const anchorRowW = anchorRowCount * cellW + (anchorRowCount - 1) * gap;
    const naiveAnchorSlotCenterX = -anchorRowW / 2 + anchorIndexInRow * stepX + cellW / 2;
    const gridShiftX = ax - naiveAnchorSlotCenterX;

    for (let i = 0; i < n; i++) {
      let row: number;
      let indexInRow: number;
      let countInRow: number;
      if (i < partialCount) {
        row = 0;
        indexInRow = i;
        countInRow = partialCount;
      } else {
        const offset = i - partialCount;
        row = 1 + Math.floor(offset / cols);
        indexInRow = offset % cols;
        countInRow = cols;
      }
      const rowW = countInRow * cellW + (countInRow - 1) * gap;
      const rowOriginX = -rowW / 2 + gridShiftX;
      result[i].x = rowOriginX + indexInRow * stepX;
      result[i].y = naiveGridOriginY + gridShiftY + row * stepY;
    }
    return result;
  }

  // Mixed-size path — shelf-pack rows centered on anchor's world center.
  // Each row's height is determined by its tallest member; rows are
  // accumulated in the windows' existing order to keep the anchor near its
  // natural row. Row width budget is the width of N cells of the widest
  // window — keeps the result roughly square.
  return organizeShelfPack(result, anchor, gap);
}

function organizeShelfPack(
  result: WindowRect[],
  anchor: WindowRect,
  gap: number
): WindowRect[] {
  const widestW = result.reduce((m, w) => Math.max(m, w.w), 0);
  const tallestH = result.reduce((m, w) => Math.max(m, w.h), 0);
  // Pick row-width budget that produces a square-ish grid given mixed sizes.
  const n = result.length;
  const colsHint = Math.max(1, chooseCols(n));
  const budgetW = colsHint * widestW + (colsHint - 1) * gap;

  interface Shelf {
    windows: WindowRect[];
    widthUsed: number;
    rowH: number;
  }
  const shelves: Shelf[] = [];
  for (const w of result) {
    const last = shelves[shelves.length - 1];
    const fits = last && last.widthUsed + gap + w.w <= budgetW;
    if (fits) {
      last!.windows.push(w);
      last!.widthUsed += gap + w.w;
      last!.rowH = Math.max(last!.rowH, w.h);
    } else {
      shelves.push({ windows: [w], widthUsed: w.w, rowH: w.h });
    }
  }

  const ax = anchor.x + anchor.w / 2;
  const ay = anchor.y + anchor.h / 2;
  // Find anchor's shelf so we can place the grid such that anchor's row
  // center is at the anchor's current world center.
  let anchorShelf = 0;
  let anchorIndexInShelf = 0;
  outer: for (let i = 0; i < shelves.length; i++) {
    const idx = shelves[i].windows.findIndex((w) => w.id === anchor.id);
    if (idx >= 0) {
      anchorShelf = i;
      anchorIndexInShelf = idx;
      break outer;
    }
  }
  // Anchor's shelf vertical center
  let yAcc = 0;
  for (let i = 0; i < anchorShelf; i++) yAcc += shelves[i].rowH + gap;
  const anchorShelfTop = yAcc;
  const anchorShelfCenterY = anchorShelfTop + shelves[anchorShelf].rowH / 2;
  const shiftY = ay - anchorShelfCenterY;
  // Anchor's horizontal center within its shelf
  let xAcc = 0;
  for (let i = 0; i < anchorIndexInShelf; i++) {
    xAcc += shelves[anchorShelf].windows[i].w + gap;
  }
  const anchorCenterXLocal = xAcc + anchor.w / 2;
  // We center each shelf around anchor's center: shelfOriginX = ax - shelfW/2.
  // For the anchor shelf, place the anchor exactly at (ax, ay) — shift shelf
  // so anchor lines up. Other shelves just center under the anchor.
  void tallestH;

  let y = shiftY;
  for (let si = 0; si < shelves.length; si++) {
    const s = shelves[si];
    const shelfW = s.widthUsed; // already widthUsed = sum widths + gaps
    let shelfOriginX = ax - shelfW / 2;
    if (si === anchorShelf) {
      // Override: align anchor.center.x = ax inside this row
      shelfOriginX = ax - anchorCenterXLocal;
    }
    let xCursor = shelfOriginX;
    for (const w of s.windows) {
      w.x = xCursor;
      w.y = y + (s.rowH - w.h) / 2; // vertically center within row
      xCursor += w.w + gap;
    }
    y += s.rowH + gap;
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
