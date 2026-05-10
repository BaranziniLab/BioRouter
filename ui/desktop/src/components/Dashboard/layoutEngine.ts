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

const Z_TILED = 1;
const Z_OVERFLOW = 50;
const Z_FOCUSED = 100;
const TARGET_ASPECT = 1.3;

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

function intersectionCandidates(
  cols: number,
  rows: number,
  cellW: number,
  cellH: number,
  board: BoardSize
): Array<{ x: number; y: number }> {
  const xs: number[] = [];
  const ys: number[] = [];
  for (let i = 0; i <= cols; i++) xs.push(i * cellW);
  for (let j = 0; j <= rows; j++) ys.push(j * cellH);
  const center = { x: board.width / 2, y: board.height / 2 };
  const points: Array<{ x: number; y: number; dist: number }> = [];
  for (const x of xs) {
    for (const y of ys) {
      const dist = Math.hypot(x - center.x, y - center.y);
      points.push({ x, y, dist });
    }
  }
  points.sort((p1, p2) => p1.dist - p2.dist);
  const deduped: Array<{ x: number; y: number }> = [];
  for (const p of points) {
    const tooClose = deduped.some((q) => Math.abs(q.x - p.x) < 40 && Math.abs(q.y - p.y) < 30);
    if (!tooClose) deduped.push({ x: p.x, y: p.y });
  }
  return deduped;
}

export function computeLayout(
  windows: readonly LayoutInputWindow[],
  board: BoardSize,
  T1: number,
  T2: number,
  focusedWindowId: string | null
): Map<string, LayoutRect> {
  void T2; // overflow vs tuck selection happens upstream; we just lay out whatever isn't tucked
  const out = new Map<string, LayoutRect>();

  const visible = windows.filter((w) => !w.isTucked);

  const manual = visible.filter((w) => w.isManuallyPlaced && w.position && w.size);
  const auto = visible.filter((w) => !(w.isManuallyPlaced && w.position && w.size));

  for (const w of manual) {
    out.set(w.windowId, {
      x: w.position!.x,
      y: w.position!.y,
      w: w.size!.w,
      h: w.size!.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : Z_TILED + 5,
    });
  }

  if (auto.length === 0) return out;

  const tiled = auto.slice(0, Math.min(auto.length, T1));
  const overflow = auto.slice(tiled.length);

  const { cols, rows } = bestGridConfig(tiled.length, board);
  const cellW = board.width / cols;
  const cellH = board.height / rows;

  for (let i = 0; i < tiled.length; i++) {
    const row = Math.floor(i / cols);
    const col = i % cols;
    const isLastRow = row === rows - 1;
    const itemsInLastRow = tiled.length - cols * (rows - 1);
    let x = col * cellW;
    const y = row * cellH;
    if (isLastRow && itemsInLastRow < cols) {
      const lastRowOffset = (board.width - itemsInLastRow * cellW) / 2;
      x = lastRowOffset + col * cellW;
    }
    out.set(tiled[i].windowId, {
      x,
      y,
      w: cellW,
      h: cellH,
      zIndex: tiled[i].windowId === focusedWindowId ? Z_FOCUSED : Z_TILED,
    });
  }

  if (overflow.length > 0) {
    const candidates = intersectionCandidates(cols, rows, cellW, cellH, board);
    // Cap overflow size to half the board so degenerate T1=1 (where cellW == board.width)
    // still leaves room to position cells distinctly. T1-cell remains the minimum elsewhere.
    const overflowW = Math.min(cellW, board.width * 0.5);
    const overflowH = Math.min(cellH, board.height * 0.5);
    for (let i = 0; i < overflow.length; i++) {
      const base = candidates[i] ?? { x: board.width / 2, y: board.height / 2 };
      const jitter = i * 8;
      const cx = base.x + jitter;
      const cy = base.y + jitter;
      const x = Math.max(0, Math.min(board.width - overflowW, cx - overflowW / 2));
      const y = Math.max(0, Math.min(board.height - overflowH, cy - overflowH / 2));
      out.set(overflow[i].windowId, {
        x,
        y,
        w: overflowW,
        h: overflowH,
        zIndex: overflow[i].windowId === focusedWindowId ? Z_FOCUSED : Z_OVERFLOW + i,
      });
    }
  }

  return out;
}
