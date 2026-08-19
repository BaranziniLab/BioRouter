/**
 * The knowledge-graph palette solver — ui-spec.md §5.2.
 *
 * Turns the ~50 numbers in `themes/graph.mjs` into the 28 type fills, the 7
 * credibility ring hues and the shape map, against a ground the CALLER
 * resolves. This module never names a ground and never names a hex.
 *
 * THE RULE IS NORMATIVE; THE HEX TABLES IN §5.3 ARE ITS PINNED OUTPUT. If this
 * solver ever produces a different last bit, the table is corrected from the
 * measurement, never the other way round — and `graphPalette.test.ts` pins the
 * output so that correction can never happen SILENTLY.
 *
 * Working space is OKLCH. Four conventions decide the last bit, and they are
 * reproduced here exactly because a different choice shifts perhaps a fifth of
 * the palette by one 8-bit step:
 *
 *   1. Bisection on `L ∈ [0.05, 0.99]`, 50 iterations.
 *   2. At each probe the chroma is gamut-mapped by bisection on C, 24
 *      iterations, in-gamut tolerance ±1e-4 on LINEAR RGB.
 *   3. The result is rounded to 8 bits BEFORE the ratio is taken, so the
 *      measured value is the shipped value.
 *   4. On a LIGHT ground contrast falls as L rises, so the solver keeps the
 *      largest L whose ratio ≥ target. On a DARK ground contrast rises with L,
 *      and it keeps the largest L whose ratio ≤ target — which is why the dark
 *      floor measures 3.48 against a 3.50 nominal rather than 3.50 or above.
 *
 * There is a SECOND implementation of this solve, in TypeScript, at
 * `src/styles/graphPalette.ts` — the runtime needs it for the DR-11 hashed
 * fallback, which is computed lazily per unseen type and must not be
 * precomputed for 360 hues. The two are held byte-identical by an explicit
 * cross-implementation sweep in `src/styles/graphPalette.test.ts`; that test is
 * the reason two implementations are acceptable here.
 */
import { contrast } from './theme-tokens.mjs';

/* ── OKLCH → sRGB ── */

const encode = (v) => (v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055);

/** OKLab → linear sRGB (Ottosson). */
function oklabToLinear(L, a, b) {
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
const inGamut = (rgb) => rgb.every((v) => v >= -GAMUT_TOL && v <= 1 + GAMUT_TOL);

/**
 * The 8-bit hex for an OKLCH triple, with chroma reduced until the colour fits
 * sRGB.
 *
 * Reducing chroma rather than clipping the channels is what keeps hue and
 * lightness intact: channel clipping would silently change BOTH, and the
 * contrast the solver then measures would not be the contrast of the colour it
 * asked for.
 */
export function oklchToHex(L, chroma, hueDeg) {
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

/**
 * The hex at `hue`/`chroma` whose contrast against `ground` hits `rung`.
 *
 * `mode` is not cosmetic: it selects the direction of the search, because
 * contrast is monotone in L with OPPOSITE sign on a light and a dark ground.
 * Passing the wrong one does not fail — it returns the far end of the ramp.
 */
export function solveHex(hue, chroma, rung, ground, mode) {
  const satisfies =
    mode === 'light' ? (ratio) => ratio >= rung : /* dark */ (ratio) => ratio <= rung;
  let lo = 0.05;
  let hi = 0.99;
  for (let i = 0; i < 50; i++) {
    const mid = (lo + hi) / 2;
    if (satisfies(contrast(oklchToHex(mid, chroma, hue), ground))) lo = mid;
    else hi = mid;
  }
  return oklchToHex(lo, chroma, hue);
}

/**
 * The hue of member `i` of an `n`-member family, spread evenly about the
 * anchor. A one-member family would take the anchor itself.
 */
export const memberHue = (anchorHue, spread, i, n) =>
  n === 1 ? anchorHue : anchorHue + (i / (n - 1) - 0.5) * spread;

/**
 * Build one mode's palette against a resolved ground.
 *
 * `ground` is the resolved `--background-muted` for the mode. This function
 * does not know which token that is, which is the point: the ground is the
 * caller's to resolve and the solver's to measure against.
 */
export function buildGraphPalette(spec, ground, mode) {
  const {
    FAMILIES,
    PRIMARY_RUNGS,
    PROVENANCE_RUNGS,
    FALLBACK_CHROMA,
    FALLBACK_RUNGS,
    CREDIBILITY,
    NODE_SHAPES,
  } = spec;

  const types = {};
  const families = {};
  const shapeOf = {};

  for (const family of FAMILIES) {
    const rungs = family.ladder === 'provenance' ? PROVENANCE_RUNGS : PRIMARY_RUNGS;
    if (family.members.length > rungs.length) {
      throw new Error(
        `graph palette: family "${family.name}" has ${family.members.length} members but its ` +
          `"${family.ladder}" ladder has only ${rungs.length} rungs`
      );
    }
    if (!NODE_SHAPES.includes(family.shape)) {
      throw new Error(`graph palette: family "${family.name}" has unknown shape "${family.shape}"`);
    }
    families[family.name] = { shape: family.shape, members: [...family.members] };
    family.members.forEach((type, i) => {
      if (type in types) {
        throw new Error(`graph palette: type "${type}" is declared in more than one family`);
      }
      const hue = memberHue(family.anchorHue, family.spread, i, family.members.length);
      types[type] = solveHex(hue, family.chroma, rungs[i], ground, mode);
      shapeOf[type] = family.shape;
    });
  }

  const credibility = {};
  const ringArcs = {};
  for (const row of CREDIBILITY) {
    credibility[row.tier] = solveHex(row.hue, row.chroma, row.rung, ground, mode);
    ringArcs[row.tier] = row.arcs;
  }

  return {
    types,
    families,
    shapeOf,
    credibility,
    ringArcs,
    fallbackChroma: FALLBACK_CHROMA,
    fallbackRungs: [...FALLBACK_RUNGS],
    ground,
  };
}
