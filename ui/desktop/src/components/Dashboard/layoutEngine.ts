export interface LayoutInputWindow {
  windowId: string;
  isManuallyPlaced: boolean;
  isTucked: boolean;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  lastInteraction: number;
}

export interface BoardSize {
  width: number;
  height: number;
}

export interface LayoutRect {
  x: number;
  y: number;
  w: number;
  h: number;
  zIndex: number;
}

export const Z_TILED = 1;
const Z_OVERFLOW = 50;
const Z_FOCUSED = 100;
const Z_PINNED = 5;
const TARGET_ASPECT = 1.3;
export const COMFORT_W = 940;
export const COMFORT_H = 800;
const MIN_W = 320;
const MIN_H = 240;
export const EDGE_INSET = 6;
const FILL_FACTOR_MAX = 0.7;
const RELAX_PASSES = 6;
const RELAX_STEP_MAX = 20;
export const SNAP_GRID = 4;

// Stage 3 overflow-placement tuning.
const OVERFLOW_DEDUP_X = 40; // px horizontal threshold to consider two intersection candidates "the same"
const OVERFLOW_DEDUP_Y = 30; // px vertical threshold for the same
const OVERFLOW_CELL_FRACTION = 0.5; // overflow cell capped at this fraction of interior
const OVERFLOW_JITTER_STRIDE = 8; // px deterministic jitter per overflow index

// Stage 5 relaxation tuning.
const RELAX_OVERLAP_SCALE = 0.5; // push magnitude = sqrt(overlap area) * RELAX_OVERLAP_SCALE

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function overlap(a: Rect, b: Rect): { w: number; h: number; area: number } {
  const oxL = Math.max(a.x, b.x);
  const oxR = Math.min(a.x + a.w, b.x + b.w);
  const oyT = Math.max(a.y, b.y);
  const oyB = Math.min(a.y + a.h, b.y + b.h);
  const w = Math.max(0, oxR - oxL);
  const h = Math.max(0, oyB - oyT);
  return { w, h, area: w * h };
}

export function hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = (h * 0x01000193) >>> 0;
  }
  return h >>> 0;
}

function bestGridConfig(n: number, board: BoardSize): { cols: number; rows: number } {
  let best = { cols: n, rows: 1, score: Infinity };
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols);
    const cellW = board.width / cols;
    const cellH = board.height / rows;
    const aspect = cellW / cellH;
    const score = Math.abs(Math.log(aspect) - Math.log(TARGET_ASPECT));
    if (score < best.score) best = { cols, rows, score };
  }
  return { cols: best.cols, rows: best.rows };
}

export function computeLayout(
  windows: readonly LayoutInputWindow[],
  board: BoardSize,
  T1: number,
  T2: number,
  focusedWindowId: string | null,
  /** When provided, every auto window is rendered at exactly this size. Skips
   * the fillFactor-based scaling — used by the Dashboard so windows always
   * spawn at the spring-back minimum size. */
  fixedCellSize?: { w: number; h: number }
): Map<string, LayoutRect> {
  void T2;
  const out = new Map<string, LayoutRect>();

  // -------- Stage 1: Partition --------
  // Stable sort by windowId so input ordering doesn't affect output.
  const visible = windows
    .filter((w) => !w.isTucked)
    .slice()
    .sort((a, b) => (a.windowId < b.windowId ? -1 : a.windowId > b.windowId ? 1 : 0));

  // A window is "pinned" when the user has explicitly moved or resized it. Either
  // position or size may be set independently (drag sets position only; resize sets
  // size only). Fill in the missing field with comfort defaults so the engine has
  // a complete rect to work with.
  const pinned = visible.filter((w) => w.isManuallyPlaced && (w.position || w.size));
  const auto = visible.filter((w) => !(w.isManuallyPlaced && (w.position || w.size)));

  const pinnedRects: Array<{ id: string; x: number; y: number; w: number; h: number }> = [];
  for (const w of pinned) {
    const r = {
      id: w.windowId,
      x: w.position?.x ?? EDGE_INSET,
      y: w.position?.y ?? EDGE_INSET,
      w: w.size?.w ?? COMFORT_W,
      h: w.size?.h ?? COMFORT_H,
    };
    pinnedRects.push(r);
    out.set(w.windowId, {
      x: r.x,
      y: r.y,
      w: r.w,
      h: r.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : Z_PINNED,
    });
  }

  if (auto.length === 0) return out;

  // -------- Stage 2: Cell size --------
  const interiorW = Math.max(MIN_W, board.width - 2 * EDGE_INSET);
  const interiorH = Math.max(MIN_H, board.height - 2 * EDGE_INSET);
  const pinnedArea = pinnedRects.reduce((s, r) => s + r.w * r.h, 0);
  const availableArea = Math.max(MIN_W * MIN_H, interiorW * interiorH - pinnedArea);
  let cellW: number;
  let cellH: number;
  if (fixedCellSize) {
    // Caller dictates the cell size (Dashboard uses min-window size). Skip the
    // comfort-vs-fill scaling — every auto window renders at exactly this size.
    cellW = fixedCellSize.w;
    cellH = fixedCellSize.h;
  } else {
    const totalComfort = auto.length * COMFORT_W * COMFORT_H;
    const fillFactor = totalComfort / availableArea;
    cellW = COMFORT_W;
    cellH = COMFORT_H;
    if (fillFactor > FILL_FACTOR_MAX) {
      const s = Math.sqrt(FILL_FACTOR_MAX / fillFactor);
      cellW = Math.max(MIN_W, Math.floor(COMFORT_W * s));
      cellH = Math.max(MIN_H, Math.floor(COMFORT_H * s));
    }
  }

  // -------- Stage 3: Initial slot placement --------
  const autoPos = new Map<string, { x: number; y: number; w: number; h: number; zCat: number }>();

  if (!fixedCellSize && auto.length <= 2) {
    // Comfort row — cells stay at comfort size regardless of fillFactor.
    const rowW = COMFORT_W;
    const rowH = COMFORT_H;
    const total = auto.length * rowW;
    const gap = (interiorW - total) / (auto.length + 1);
    const y = EDGE_INSET + Math.max(0, (interiorH - rowH) / 2);
    for (let i = 0; i < auto.length; i++) {
      const x = EDGE_INSET + gap + i * (rowW + gap);
      autoPos.set(auto[i].windowId, { x, y, w: rowW, h: rowH, zCat: Z_TILED });
    }
  } else {
    const nTiled = Math.min(auto.length, T1);
    const { cols, rows } = bestGridConfig(nTiled, { width: interiorW, height: interiorH });
    // When fixedCellSize is provided, every cell is exactly that size (Dashboard
    // pins windows at the spring-back minimum). Otherwise, snap the auto-cell
    // size to SNAP_GRID so post-snap cells remain edge-aligned with no drift.
    const sizedW = fixedCellSize
      ? cellW
      : Math.max(
          MIN_W,
          Math.floor(Math.min(cellW, Math.floor(interiorW / cols)) / SNAP_GRID) * SNAP_GRID
        );
    const sizedH = fixedCellSize
      ? cellH
      : Math.max(
          MIN_H,
          Math.floor(Math.min(cellH, Math.floor(interiorH / rows)) / SNAP_GRID) * SNAP_GRID
        );
    const itemsInLastRow = nTiled - cols * (rows - 1);
    const blockW = cols * sizedW;
    const blockH = rows * sizedH;
    const baseX = EDGE_INSET + Math.max(0, (interiorW - blockW) / 2);
    const baseY = EDGE_INSET + Math.max(0, (interiorH - blockH) / 2);

    for (let i = 0; i < nTiled; i++) {
      const row = Math.floor(i / cols);
      const col = i % cols;
      const isLastRow = row === rows - 1;
      let x = baseX + col * sizedW;
      const y = baseY + row * sizedH;
      if (isLastRow && itemsInLastRow < cols) {
        const lastRowOffset = (blockW - itemsInLastRow * sizedW) / 2;
        x = baseX + lastRowOffset + col * sizedW;
      }
      autoPos.set(auto[i].windowId, { x, y, w: sizedW, h: sizedH, zCat: Z_TILED });
    }

    const overflow = auto.slice(nTiled);
    if (overflow.length > 0) {
      const xs: number[] = [];
      const ys: number[] = [];
      for (let i = 0; i <= cols; i++) xs.push(baseX + i * sizedW);
      for (let j = 0; j <= rows; j++) ys.push(baseY + j * sizedH);
      const center = { x: baseX + blockW / 2, y: baseY + blockH / 2 };
      type C = { x: number; y: number; dist: number };
      const cand: C[] = [];
      for (const x of xs) {
        for (const y of ys) {
          cand.push({ x, y, dist: Math.hypot(x - center.x, y - center.y) });
        }
      }
      cand.sort((p, q) => p.dist - q.dist);
      const deduped: Array<{ x: number; y: number }> = [];
      for (const p of cand) {
        const tooClose = deduped.some(
          (q) => Math.abs(q.x - p.x) < OVERFLOW_DEDUP_X && Math.abs(q.y - p.y) < OVERFLOW_DEDUP_Y
        );
        if (!tooClose) deduped.push(p);
      }
      const ofW = Math.min(sizedW, Math.floor(interiorW * OVERFLOW_CELL_FRACTION));
      const ofH = Math.min(sizedH, Math.floor(interiorH * OVERFLOW_CELL_FRACTION));
      for (let i = 0; i < overflow.length; i++) {
        const base = deduped[i] ?? { x: center.x, y: center.y };
        const jitter = i * OVERFLOW_JITTER_STRIDE;
        const cx = base.x + jitter;
        const cy = base.y + jitter;
        const x = Math.max(EDGE_INSET, Math.min(board.width - EDGE_INSET - ofW, cx - ofW / 2));
        const y = Math.max(EDGE_INSET, Math.min(board.height - EDGE_INSET - ofH, cy - ofH / 2));
        autoPos.set(overflow[i].windowId, { x, y, w: ofW, h: ofH, zCat: Z_OVERFLOW + i });
      }
    }
  }

  // -------- Stage 4: Repulse auto-vs-pinned --------
  for (const w of auto) {
    const r = autoPos.get(w.windowId);
    if (!r) continue;
    for (const p of pinnedRects) {
      const o = overlap(r, p);
      if (o.area === 0) continue;
      const minX = EDGE_INSET;
      const maxX = board.width - EDGE_INSET - r.w;
      const minY = EDGE_INSET;
      const maxY = board.height - EDGE_INSET - r.h;
      const exits = [
        { dx: p.x - (r.x + r.w), dy: 0 }, // west
        { dx: p.x + p.w - r.x, dy: 0 }, // east
        { dx: 0, dy: p.y - (r.y + r.h) }, // north
        { dx: 0, dy: p.y + p.h - r.y }, // south
      ];
      // Mark each exit as feasible (the move lands inside the board without clamping).
      const annotated = exits.map((e) => {
        const nx = r.x + e.dx;
        const ny = r.y + e.dy;
        const feasible = nx >= minX && nx <= maxX && ny >= minY && ny <= maxY;
        return { ...e, feasible, mag: Math.abs(e.dx) + Math.abs(e.dy) };
      });
      // Sort: feasible first, then by magnitude ascending.
      annotated.sort((a, b) => {
        if (a.feasible !== b.feasible) return a.feasible ? -1 : 1;
        return a.mag - b.mag;
      });
      let pick = annotated[0];
      // Deterministic tie-break only when the two top candidates have equal feasibility and magnitude.
      if (
        annotated.length > 1 &&
        annotated[0].feasible === annotated[1].feasible &&
        annotated[0].mag === annotated[1].mag
      ) {
        pick = (hash32(w.windowId) & 1) === 0 ? annotated[0] : annotated[1];
      }
      r.x = Math.max(minX, Math.min(maxX, r.x + pick.dx));
      r.y = Math.max(minY, Math.min(maxY, r.y + pick.dy));
    }
  }

  // -------- Stage 5: Relaxation --------
  // Pinned rects participate as immovable obstacles so auto-vs-auto repulsion never
  // pushes an auto rect back into a pinned region.
  for (let pass = 0; pass < RELAX_PASSES; pass++) {
    const delta = new Map<string, { dx: number; dy: number }>();
    for (let i = 0; i < auto.length; i++) {
      for (let j = i + 1; j < auto.length; j++) {
        const a = autoPos.get(auto[i].windowId);
        const b = autoPos.get(auto[j].windowId);
        if (!a || !b) continue;
        const o = overlap(a, b);
        if (o.area === 0) continue;
        const cxA = a.x + a.w / 2;
        const cyA = a.y + a.h / 2;
        const cxB = b.x + b.w / 2;
        const cyB = b.y + b.h / 2;
        let vx = cxA - cxB;
        let vy = cyA - cyB;
        let len = Math.hypot(vx, vy);
        if (len === 0) {
          const h = hash32(auto[i].windowId + '|' + auto[j].windowId);
          const angle = ((h & 0xff) / 256) * Math.PI * 2;
          vx = Math.cos(angle);
          vy = Math.sin(angle);
          len = 1;
        }
        const mag = Math.min(RELAX_STEP_MAX, Math.sqrt(o.area) * RELAX_OVERLAP_SCALE);
        const ux = vx / len;
        const uy = vy / len;
        const dA = delta.get(auto[i].windowId) ?? { dx: 0, dy: 0 };
        dA.dx += ux * mag;
        dA.dy += uy * mag;
        delta.set(auto[i].windowId, dA);
        const dB = delta.get(auto[j].windowId) ?? { dx: 0, dy: 0 };
        dB.dx -= ux * mag;
        dB.dy -= uy * mag;
        delta.set(auto[j].windowId, dB);
      }
      // Auto vs pinned: auto absorbs the full push; pinned does not move.
      const a = autoPos.get(auto[i].windowId);
      if (!a) continue;
      // Why Stage 5 also pushes autos out of pinned overlap:
      // Stage 4 placed each auto at its nearest feasible exit from any pinned overlap it
      // started with. But during Stage 5's auto-vs-auto relaxation, an auto can be pushed
      // back into pinned territory by a neighboring auto. This inner pass keeps autos out
      // of pinned regions even as they jostle each other. Pinned rects never move.
      for (const p of pinnedRects) {
        const o = overlap(a, p);
        if (o.area === 0) continue;
        const cxA = a.x + a.w / 2;
        const cyA = a.y + a.h / 2;
        const cxP = p.x + p.w / 2;
        const cyP = p.y + p.h / 2;
        let vx = cxA - cxP;
        let vy = cyA - cyP;
        let len = Math.hypot(vx, vy);
        if (len === 0) {
          const h = hash32(auto[i].windowId + '|' + p.id);
          const angle = ((h & 0xff) / 256) * Math.PI * 2;
          vx = Math.cos(angle);
          vy = Math.sin(angle);
          len = 1;
        }
        // Apply at full magnitude (auto absorbs all push since pinned is immovable).
        const mag = Math.min(RELAX_STEP_MAX, Math.sqrt(o.area));
        const ux = vx / len;
        const uy = vy / len;
        const dA = delta.get(auto[i].windowId) ?? { dx: 0, dy: 0 };
        dA.dx += ux * mag;
        dA.dy += uy * mag;
        delta.set(auto[i].windowId, dA);
      }
    }
    for (const w of auto) {
      const r = autoPos.get(w.windowId);
      const d = delta.get(w.windowId);
      if (!r || !d) continue;
      r.x = Math.max(EDGE_INSET, Math.min(board.width - EDGE_INSET - r.w, r.x + d.dx));
      r.y = Math.max(EDGE_INSET, Math.min(board.height - EDGE_INSET - r.h, r.y + d.dy));
    }
  }

  // -------- Stage 6: Snap & emit --------
  // Snap downward so adjacent cells don't get pushed into each other; then clamp into
  // the edge-inset bounds.
  for (const w of auto) {
    const r = autoPos.get(w.windowId);
    if (!r) continue;
    let sx = Math.floor(r.x / SNAP_GRID) * SNAP_GRID;
    let sy = Math.floor(r.y / SNAP_GRID) * SNAP_GRID;
    const minX = EDGE_INSET;
    const maxX = board.width - EDGE_INSET - r.w;
    const minY = EDGE_INSET;
    const maxY = board.height - EDGE_INSET - r.h;
    if (sx < minX) sx = minX;
    if (sx > maxX) sx = maxX;
    if (sy < minY) sy = minY;
    if (sy > maxY) sy = maxY;
    out.set(w.windowId, {
      x: sx,
      y: sy,
      w: r.w,
      h: r.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : r.zCat,
    });
  }

  return out;
}
