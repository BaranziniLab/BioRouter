/**
 * Reading the knowledge-graph palette, and solving the DR-11 fallback.
 *
 * The 28 curated fills, the 7 ring hues and the shape map are GENERATED into
 * `themes.generated.ts` from `themes/graph.mjs`; this module is the read side,
 * plus the one thing that cannot be generated — a colour for an arbitrary OKF
 * type string.
 *
 * WHY THE FALLBACK IS SOLVED AT RUNTIME RATHER THAN PRECOMPUTED. OKF mode
 * allows any `type` string, so the palette cannot be a lookup table alone. The
 * hue space is 360 × 4 rungs × 2 modes, and a base has O(10) distinct types —
 * precomputing 2,880 hexes to use ten of them would put a table in the bundle
 * to avoid a bisection that is paid once per type per session. It is memoised
 * on `type|mode` instead.
 *
 * ⚠ THIS FILE CARRIES A SECOND COPY OF THE §5.2 SOLVER. The first is
 * `scripts/lib/graph-palette.mjs`, which the theme generator uses; a node
 * script cannot import TypeScript and the renderer cannot import the generator,
 * so the duplication is structural rather than lazy. What makes it safe is that
 * `graphPalette.test.ts` sweeps both implementations over the same hue/rung
 * space and asserts them byte-identical. If you edit one, edit both — the test
 * will tell you if you did not.
 */
import { GRAPH_PALETTE } from './themes.generated';
import type { GraphPalette, NodeShape } from './themes.generated';

export type GraphMode = 'light' | 'dark';

export type { GraphCredibilityKey, GraphPalette, NodeShape } from './themes.generated';
export { GRAPH_PALETTE } from './themes.generated';

/* ── the §5.2 solver ── */

const encode = (v: number): number =>
  v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;

/** OKLab → linear sRGB (Ottosson). */
function oklabToLinear(L: number, a: number, b: number): [number, number, number] {
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.291485548 * b;
  const l = l_ * l_ * l_;
  const m = m_ * m_ * m_;
  const s = s_ * s_ * s_;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const GAMUT_TOL = 1e-4;
const inGamut = (rgb: [number, number, number]): boolean =>
  rgb.every((v) => v >= -GAMUT_TOL && v <= 1 + GAMUT_TOL);

/**
 * The 8-bit hex for an OKLCH triple, chroma reduced until the colour fits sRGB.
 *
 * Reducing chroma rather than clipping channels is what keeps hue and lightness
 * intact — clipping would change both, and the contrast measured next would not
 * be the contrast of the colour that was asked for.
 */
export function oklchToHex(L: number, chroma: number, hueDeg: number): string {
  const h = (hueDeg * Math.PI) / 180;
  const cos = Math.cos(h);
  const sin = Math.sin(h);
  let c = chroma;
  if (!inGamut(oklabToLinear(L, c * cos, c * sin))) {
    let lo = 0;
    let hi = chroma;
    for (let i = 0; i < 24; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklabToLinear(L, mid * cos, mid * sin))) lo = mid;
      else hi = mid;
    }
    c = lo;
  }
  return (
    '#' +
    oklabToLinear(L, c * cos, c * sin)
      .map((v) => {
        const clamped = Math.min(1, Math.max(0, v));
        return Math.round(Math.min(1, Math.max(0, encode(clamped))) * 255)
          .toString(16)
          .padStart(2, '0');
      })
      .join('')
  );
}

/** WCAG 2.x relative luminance of a 6-digit hex. */
function luminance(hex: string): number {
  const c = [1, 3, 5]
    .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** WCAG 2.x contrast ratio between two 6-digit hexes. */
export function contrastRatio(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/**
 * The hex at `hue`/`chroma` whose contrast against `ground` hits `rung`.
 *
 * `mode` selects the direction of the search, and is not cosmetic: contrast is
 * monotone in L with OPPOSITE sign on a light and a dark ground. On light the
 * solver keeps the largest L whose ratio is at or above the rung; on dark, the
 * largest L whose ratio is at or below it — which is why a dark 3.50 rung
 * measures 3.48 rather than 3.50 or above.
 */
export function solveHex(
  hue: number,
  chroma: number,
  rung: number,
  ground: string,
  mode: GraphMode
): string {
  const satisfies =
    mode === 'light' ? (ratio: number) => ratio >= rung : (ratio: number) => ratio <= rung;
  let lo = 0.05;
  let hi = 0.99;
  for (let i = 0; i < 50; i++) {
    const mid = (lo + hi) / 2;
    if (satisfies(contrastRatio(oklchToHex(mid, chroma, hue), ground))) lo = mid;
    else hi = mid;
  }
  return oklchToHex(lo, chroma, hue);
}

/* ── DR-11: a deterministic colour for an arbitrary OKF type ── */

/**
 * FNV-1a, 32-bit, unsigned — the same hash the graph renderer uses for jitter,
 * so there is one hash in the feature.
 *
 * The input is the RAW type string, byte for byte: no case folding, no
 * trimming. `Gene` and `gene` are different types to OKF, so they get different
 * colours; folding would make an exact match with a curated type invisible in
 * the UI.
 */
export function fnv1a(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

const hashedCache = new Map<string, string>();

/**
 * The fill for a type the vocabulary does not know.
 *
 * In OKF mode essentially every node takes this path, so it must look NATIVE,
 * not like an error state — which is also why such a node is drawn as a plain
 * circle rather than given a hollow or donut marker. A marker that applies to
 * every node in the base carries no information and only costs fill-rate.
 *
 * The chroma (0.055) sits deliberately between Provenance (0.030) and every
 * biological family (0.090–0.145): an unrecognised type reads as quieter than a
 * curated biological family and more coloured than provenance.
 *
 * ⚠ WHAT THIS GUARANTEES, EXACTLY. Measured over all 1,440 (hue, rung)
 * combinations, the closest a hashed colour comes to any of the 28 curated
 * fills is ΔE00 3.50 light / 5.05 dark. ΔE00 3.50 is a subtle-but-nonzero
 * difference, NOT a guarantee of distinguishability; the guarantee is only that
 * no arbitrary string can exactly reproduce a curated colour. (With the naive
 * scheme — chroma 0.075 on the curated rungs — the measured closest approach
 * was 0.00: an exact collision at hue 207 with `BiologicalFunction`.)
 */
export function hashedFill(type: string, mode: GraphMode): string {
  const key = `${type}|${mode}`;
  const cached = hashedCache.get(key);
  if (cached !== undefined) return cached;
  const palette = GRAPH_PALETTE[mode];
  const h = fnv1a(type);
  // R-05: placed at a lightness like every curated fill, not solved to a
  // contrast ratio. `solveHex` is retained above because the credibility ring
  // still uses it — a 1.6px stroke IS asked to carry contrast, where a fill is
  // not.
  const hex = oklchToHex(
    palette.fallbackLightness[(h >>> 9) & 3],
    palette.fallbackChroma,
    h % 360
  );
  hashedCache.set(key, hex);
  return hex;
}

/* ── the read side ── */

/** The fill for a node type: curated when the vocabulary knows it, hashed otherwise. */
export function typeFill(type: string, mode: GraphMode): string {
  return GRAPH_PALETTE[mode].types[type] ?? hashedFill(type, mode);
}

/**
 * The silhouette for a node type.
 *
 * Falls back to `circle`, which is also what every node in an OKF base draws:
 * there are no families there, and a shape channel that applies to everything
 * carries nothing.
 */
export function typeShape(type: string, mode: GraphMode): NodeShape {
  return GRAPH_PALETTE[mode].shapeOf[type] ?? 'circle';
}

/** The palette for a mode. */
export const paletteFor = (mode: GraphMode): GraphPalette => GRAPH_PALETTE[mode];

/**
 * Test seam. The memo is process-global and keyed by `type|mode`, which is
 * correct at runtime (a type's colour never changes within a mode) and wrong
 * across a test that swaps the palette underneath it.
 */
export function __resetHashedFillCache(): void {
  hashedCache.clear();
}
