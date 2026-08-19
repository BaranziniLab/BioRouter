// ui/desktop/src/components/knowledge/graph/nodeShapes.ts
import type { NodeShape } from '../../../styles/graphPalette';

/**
 * The seven silhouettes (ui-spec §5.3.1) — one `ctx.beginPath()` branch.
 *
 * **Shape carries FAMILY; lightness carries the member within a family.** This
 * is the redundant non-colour channel WCAG 1.4.1 requires, and it is the answer
 * to a measurement rather than a taste: cross-family colour distance under
 * simulated dichromacy bottoms out at ΔE00 **0.00** (`Phenotype`/`Food`, dark
 * tritanopia) and no palette of 28 marks can do better on one surviving opponent
 * axis. Seven shapes is about the discriminable limit for a silhouette at this
 * size — 28 would need shape to do the whole job and could not.
 *
 * ⚠ **`r` is the CIRCUMRADIUS, not an inradius or a half-width.** A triangle and
 * a circle at the same `r` occupy the same hit circle, and force-graph's shadow
 * canvas paints plain arcs at `nodeRelSize` for every shape — so hit areas stay
 * circular, slightly generous for a triangle, which is the right direction.
 */

/** Below this SCREEN diameter every node paints as a circle. */
export const SHAPE_LOD_MIN = 3.0;

const TAU = Math.PI * 2;

/**
 * A regular polygon with `sides` vertices, first vertex at `−π/2` (pointing up).
 *
 * Up-pointing is not decoration: a triangle rotated to any other phase is
 * markedly harder to tell from a diamond at 10px, and the two are one of the
 * pairs §5.3.1 relies on being "strong".
 */
function polygon(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  r: number,
  sides: number,
  phase = -Math.PI / 2
): void {
  ctx.moveTo(x + r * Math.cos(phase), y + r * Math.sin(phase));
  for (let i = 1; i < sides; i++) {
    const a = phase + (TAU * i) / sides;
    ctx.lineTo(x + r * Math.cos(a), y + r * Math.sin(a));
  }
  ctx.closePath();
}

/**
 * Lay `shape` at `(x, y)` with circumradius `r` onto the current path.
 *
 * The caller owns `beginPath()`, `fill()` and `stroke()`, so one path can be
 * both filled and outlined without being built twice.
 *
 * `screenDiameter` is `r * globalScale`. Below `SHAPE_LOD_MIN` every shape
 * collapses to a circle: a polygon under ~6px across is anti-aliased into a disc
 * anyway and the path cost is wasted. Below that zoom the canvas is a DENSITY
 * MAP rather than a type map, identification is by label and inspector, and
 * §5.5 suppresses the credibility ring in the same regime — one LOD story, not
 * two.
 */
export function pathForShape(
  ctx: CanvasRenderingContext2D,
  shape: NodeShape,
  x: number,
  y: number,
  r: number,
  screenDiameter: number
): void {
  if (screenDiameter < SHAPE_LOD_MIN || shape === 'circle') {
    ctx.arc(x, y, r, 0, TAU);
    return;
  }
  switch (shape) {
    case 'square':
      polygon(ctx, x, y, r, 4, -Math.PI / 4);
      return;
    case 'diamond':
      polygon(ctx, x, y, r, 4, -Math.PI / 2);
      return;
    case 'triangle':
      polygon(ctx, x, y, r, 3, -Math.PI / 2);
      return;
    case 'pentagon':
      polygon(ctx, x, y, r, 5, -Math.PI / 2);
      return;
    case 'hexagon':
      polygon(ctx, x, y, r, 6, -Math.PI / 2);
      return;
    case 'rounded-square': {
      // A square at the same circumradius with a corner radius of 0.3× the side,
      // which is what separates it from `square` at 10px without reading as a
      // circle. `roundRect` is Chromium 99+ and Electron ships far past it; the
      // fallback keeps jsdom and any 2D context stub from throwing.
      const half = r * Math.SQRT1_2;
      const side = half * 2;
      const radius = side * 0.3;
      if (typeof ctx.roundRect === 'function') {
        ctx.roundRect(x - half, y - half, side, side, radius);
      } else {
        polygon(ctx, x, y, r, 4, -Math.PI / 4);
      }
      return;
    }
    default:
      ctx.arc(x, y, r, 0, TAU);
  }
}

/**
 * The same silhouettes as an SVG path, for the DOM.
 *
 * The legend and the facet rows teach the shape channel, so they must draw the
 * SAME shapes the canvas draws — a legend whose glyph disagrees with the mark is
 * worse than no legend. The box is `0 0 16 16`, centred, circumradius 7.
 */
export function svgPathForShape(shape: NodeShape): string {
  const cx = 8;
  const cy = 8;
  const r = 7;
  const pts = (sides: number, phase: number) =>
    Array.from({ length: sides }, (_, i) => {
      const a = phase + (TAU * i) / sides;
      return `${(cx + r * Math.cos(a)).toFixed(2)},${(cy + r * Math.sin(a)).toFixed(2)}`;
    }).join(' L');

  switch (shape) {
    case 'square':
      return `M${pts(4, -Math.PI / 4)}Z`;
    case 'diamond':
      return `M${pts(4, -Math.PI / 2)}Z`;
    case 'triangle':
      return `M${pts(3, -Math.PI / 2)}Z`;
    case 'pentagon':
      return `M${pts(5, -Math.PI / 2)}Z`;
    case 'hexagon':
      return `M${pts(6, -Math.PI / 2)}Z`;
    case 'rounded-square': {
      const half = r * Math.SQRT1_2;
      const side = half * 2;
      const rad = side * 0.3;
      const x0 = cx - half;
      const y0 = cy - half;
      return (
        `M${(x0 + rad).toFixed(2)},${y0.toFixed(2)}` +
        `h${(side - 2 * rad).toFixed(2)}a${rad.toFixed(2)},${rad.toFixed(2)} 0 0 1 ${rad.toFixed(2)},${rad.toFixed(2)}` +
        `v${(side - 2 * rad).toFixed(2)}a${rad.toFixed(2)},${rad.toFixed(2)} 0 0 1 -${rad.toFixed(2)},${rad.toFixed(2)}` +
        `h-${(side - 2 * rad).toFixed(2)}a${rad.toFixed(2)},${rad.toFixed(2)} 0 0 1 -${rad.toFixed(2)},-${rad.toFixed(2)}` +
        `v-${(side - 2 * rad).toFixed(2)}a${rad.toFixed(2)},${rad.toFixed(2)} 0 0 1 ${rad.toFixed(2)},-${rad.toFixed(2)}Z`
      );
    }
    case 'circle':
    default:
      return `M${cx},${cy - r}a${r},${r} 0 1 0 0,${2 * r}a${r},${r} 0 1 0 0,-${2 * r}Z`;
  }
}
