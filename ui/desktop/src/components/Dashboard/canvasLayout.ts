export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface WindowRect extends Rect {
  id: string;
}

function overlap(a: Rect, b: Rect): { w: number; h: number } {
  const w = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const h = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return { w, h };
}

interface SpawnParams {
  center: { x: number; y: number };
  size: { w: number; h: number };
  existing: readonly Rect[];
  gap?: number;
}

export function findSpawnPosition({
  center,
  size,
  existing,
  gap = 16,
}: SpawnParams): { x: number; y: number } {
  // Start at the camera center (translated so the window's center sits on it).
  const baseX = center.x - size.w / 2;
  const baseY = center.y - size.h / 2;
  const stepX = size.w + gap;
  const stepY = size.h + gap;
  const collides = (x: number, y: number) =>
    existing.some((r) => {
      const ov = overlap({ x, y, w: size.w, h: size.h }, r);
      return ov.w > 0 && ov.h > 0;
    });

  if (!collides(baseX, baseY)) return { x: baseX, y: baseY };

  // Expanding square ring; test boundary cells only at each radius.
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
  // Fallback: place just to the right of the existing bounding box.
  const maxX = existing.reduce((m, r) => Math.max(m, r.x + r.w), baseX);
  return { x: maxX + gap, y: baseY };
}

export function organize(
  windows: readonly WindowRect[],
  anchorId: string,
  gap = 16
): WindowRect[] {
  const result = windows.map((w) => ({ ...w }));
  const MAX_PASSES = 12;
  for (let pass = 0; pass < MAX_PASSES; pass++) {
    let moved = false;
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        const a = result[i];
        const b = result[j];
        const ov = overlap(a, b);
        if (ov.w <= 0 || ov.h <= 0) continue;
        // Push along the shorter overlap axis (minimal movement).
        const axis: 'x' | 'y' = ov.w < ov.h ? 'x' : 'y';
        const pushTotal = (axis === 'x' ? ov.w : ov.h) + gap;
        const aIsAnchor = a.id === anchorId;
        const bIsAnchor = b.id === anchorId;
        const aShare = aIsAnchor ? 0 : bIsAnchor ? pushTotal : pushTotal / 2;
        const bShare = bIsAnchor ? 0 : aIsAnchor ? pushTotal : pushTotal / 2;
        if (axis === 'x') {
          const aFirst = a.x <= b.x;
          a.x += aFirst ? -aShare : aShare;
          b.x += aFirst ? bShare : -bShare;
        } else {
          const aFirst = a.y <= b.y;
          a.y += aFirst ? -aShare : aShare;
          b.y += aFirst ? bShare : -bShare;
        }
        moved = true;
      }
    }
    if (!moved) break;
  }
  return result;
}
