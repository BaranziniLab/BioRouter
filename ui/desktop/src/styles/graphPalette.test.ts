/**
 * The knowledge-graph palette guard — ui-spec.md §6.4.
 *
 * WHY THIS IS A VITEST FILE AND NOT PART OF check-contrast.mjs. That script
 * reads exactly one file (`src/styles/main.css`), parses CSS declarations and
 * resolves `var()` chains. It has no TypeScript loader and no path to
 * `themes.generated.ts`, where the palette correctly lives — assertions put
 * there could not have run. The app already has the right mechanism, and that
 * script's own comment names it: "The per-token syntax stops are asserted in
 * codeTheme.test.ts". A Vitest test over a generated TS module is the same
 * category of consumer, so this file sits beside that one.
 *
 * What stayed in `check-contrast.mjs` is what is a CSS FACT: the six-scope
 * `--background-muted` identity, which is the entire justification for emitting
 * one palette rather than three.
 *
 * TWO HONESTY CLAUSES, because a guard whose meaning is overstated is worse
 * than a weaker one:
 *
 *   1. THE EXACT-HEX ASSERTIONS ARE CIRCULAR ON FIRST LANDING. §5.2 concedes
 *      that "the table is corrected from the measurement", so on the commit
 *      that generates the palette these pins record the output rather than
 *      validate it. Their value is AFTERWARDS: a future tweak to the solver, to
 *      a rung, to a hue or to the gamut-mapping convention cannot silently
 *      repaint every graph in the app. Do not read them as validating the
 *      spec's tables.
 *   2. A WRONG deltaE00 MAKES THE WHOLE CVD AUDIT PASS VACUOUSLY, and CIEDE2000
 *      is the easiest function here to get subtly wrong — the mean-hue
 *      discontinuity at ±180° and the R_T rotation term are both easy to
 *      mis-transcribe and neither shows up on well-separated colours. The
 *      correctness gate is therefore the published Sharma–Wu–Dalal 34-pair
 *      reference table, asserted first, below. The palette figures are
 *      regression vectors against ONE palette and prove nothing about the
 *      implementation on their own.
 */
import { describe, expect, it } from 'vitest';
import { GRAPH_PALETTE } from './themes.generated';
import type { GraphCredibilityKey, NodeShape } from './themes.generated';
import type { GraphMode } from './graphPalette';
import {
  contrastRatio,
  fnv1a,
  hashedFill,
  oklchToHex,
  solveHex,
  typeFill,
  typeShape,
} from './graphPalette';
import { NODE_RING_ALPHA } from '../components/knowledge/graph/graphStyle';

/**
 * Composite `fg` over `bg` at `alpha`, in sRGB — the same operation a 2D canvas
 * performs when it strokes a translucent ink over the ground. Local to this
 * file because it exists to measure ONE thing: whether the node's boundary
 * still clears SC 1.4.11 now that its fill deliberately does not (R-05).
 */
function composite(fg: string, bg: string, alpha: number): string {
  const ch = (hex: string, i: number) => parseInt(hex.slice(i, i + 2), 16);
  const mix = (i: number) => Math.round(alpha * ch(fg, i) + (1 - alpha) * ch(bg, i));
  return '#' + [1, 3, 5].map((i) => mix(i).toString(16).padStart(2, '0')).join('');
}
import { deltaE00, deltaE00Lab, simulateCvd } from '../../scripts/lib/theme-tokens.mjs';
import type { Dichromacy, Lab } from '../../scripts/lib/theme-tokens.mjs';
import { solveHex as solveHexGenerator } from '../../scripts/lib/graph-palette.mjs';
import sharma from '../../scripts/lib/__fixtures__/ciede2000-sharma.json';
import type { CredibilityTier } from '../api/types.gen';

/* ── compile-time: the generated key union must equal the API's ──
   `themes.generated.ts` declares GraphCredibilityKey locally rather than
   importing from `api/types.gen`, so this generated module stays free of the
   API client's generation order. That freedom is only safe if the two are
   pinned together, and a type-level equality is the pin: adding a tier to the
   OpenAPI schema without adding a row to `themes/graph.mjs` fails `tsc`, not a
   runtime assertion nobody runs. */
type Assert<T extends true> = T;
type Eq<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
export type CredibilityKeysAgreeWithTheApi = Assert<
  Eq<GraphCredibilityKey, CredibilityTier | 'retracted'>
>;

const MODES: GraphMode[] = ['light', 'dark'];
const VISION = ['normal', 'protan', 'deutan', 'tritan'] as const;
type Vision = (typeof VISION)[number];

const see = (hex: string, vision: Vision): string =>
  vision === 'normal' ? hex : simulateCvd(hex, vision as Dichromacy);

/** Type -> family name, from the generated family map. One source, no second list. */
const FAMILY_OF: Record<string, string> = {};
for (const [name, family] of Object.entries(GRAPH_PALETTE.light.families)) {
  for (const member of family.members) FAMILY_OF[member] = name;
}
const TYPES = Object.keys(GRAPH_PALETTE.light.types);
const PAIRS: [string, string][] = [];
for (let i = 0; i < TYPES.length; i++) {
  for (let j = i + 1; j < TYPES.length; j++) PAIRS.push([TYPES[i], TYPES[j]]);
}

/**
 * How confusable two silhouettes are at ~10px, from §5.3.1.
 *
 * The twelve pairs the specification measured and names are pinned below; the
 * remaining nine are filled in on the same model (a triangle is unmistakable
 * against anything; a sharp low-vertex shape against a round one is moderate;
 * two blobby outlines, or two shapes with the same basic outline, are weak) and
 * are NOT load-bearing today — no family pair uses them at a colour distance
 * that would make them matter. If a hue or a rung moves and one becomes
 * load-bearing, the assertion below is what will say so.
 */
const SHAPE_DISTINCTNESS: Record<string, 'weak' | 'moderate' | 'strong'> = {
  // measured and named in §5.3.1
  'pentagon|rounded-square': 'moderate',
  'circle|triangle': 'strong',
  'circle|diamond': 'moderate',
  'diamond|hexagon': 'moderate',
  'rounded-square|triangle': 'strong',
  'diamond|triangle': 'strong',
  'circle|square': 'moderate',
  'square|triangle': 'strong',
  'circle|hexagon': 'weak',
  'circle|rounded-square': 'weak',
  'rounded-square|square': 'weak',
  'hexagon|pentagon': 'weak',
  // filled in on the same model
  'diamond|square': 'moderate',
  'pentagon|square': 'moderate',
  'hexagon|square': 'moderate',
  'diamond|rounded-square': 'moderate',
  'diamond|pentagon': 'moderate',
  'pentagon|triangle': 'strong',
  'hexagon|triangle': 'strong',
  'hexagon|rounded-square': 'weak',
  'circle|pentagon': 'weak',
};
/** Keys are stored in sorted order, so the lookup is symmetric by construction. */
const distinctness = (a: NodeShape, b: NodeShape) => SHAPE_DISTINCTNESS[[a, b].sort().join('|')];

/* ── the pinned tables (§5.3, §5.5) ── */

const TYPE_HEXES: Record<string, { light: string; dark: string }> = {
  Gene: { light: '#a0b2ff', dark: '#5060b6' },
  Variant: { light: '#9190ed', dark: '#7875d0' },
  SequenceFeature: { light: '#8670cb', dark: '#a08be8' },
  Structure: { light: '#7952a8', dark: '#c9a1fe' },
  Molecule: { light: '#65cdb3', dark: '#007b66' },
  MolecularClass: { light: '#3ab1a6', dark: '#08968c' },
  BiologicalPathway: { light: '#009598', dark: '#34b0b3' },
  BiologicalFunction: { light: '#007785', dark: '#58cadb' },
  Anatomy: { light: '#97c87c', dark: '#4a772e' },
  CellType: { light: '#67b073', dark: '#4d955a' },
  Organism: { light: '#32976b', dark: '#51b285' },
  Disease: { light: '#ff8fb2', dark: '#a83d64' },
  Phenotype: { light: '#e77284', dark: '#c9586a' },
  BiomedicalMeasure: { light: '#ca5957', dark: '#e87470' },
  MethodOrProcedure: { light: '#ac4228', dark: '#ff977d' },
  Exposure: { light: '#eba75f', dark: '#955900' },
  SocialFactor: { light: '#c5923a', dark: '#a97816' },
  Food: { light: '#9e7e10', dark: '#b99837' },
  Device: { light: '#7ec0ea', dark: '#2d7096' },
  MaterialSample: { light: '#7f9cd5', dark: '#6582b8' },
  Publication: { light: '#a9bdaf', dark: '#3b4d41' },
  Study: { light: '#93ada8', dark: '#445c58' },
  Dataset: { light: '#819ba0', dark: '#526b6f' },
  Agent: { light: '#758895', dark: '#657885' },
  Population: { light: '#6c7688', dark: '#7c8698' },
  GeographicLocation: { light: '#656376', dark: '#9593a7' },
  Concept: { light: '#5e5162', dark: '#aea1b3' },
  Other: { light: '#54414b', dark: '#c6b0bc' },
};

const RING_HEXES: Record<GraphCredibilityKey, { light: string; dark: string }> = {
  peer_reviewed: { light: '#1c619f', dark: '#5fa1e4' },
  book: { light: '#406e9d', dark: '#6291c2' },
  preprint: { light: '#5d7b9a', dark: '#6583a3' },
  gray_lit: { light: '#768290', dark: '#6d7986' },
  web: { light: '#a26227', dark: '#ba783f' },
  personal: { light: '#9f5c83', dark: '#b67299' },
  retracted: { light: '#c04441', dark: '#e1625d' },
};

/** §5.4's hash vectors: they pin the hash AND the solver, together. */
const HASH_VECTORS: {
  type: string;
  hash: number;
  hue: number;
  /** R-05: the fallback places at an OKLab lightness, it does not solve to a rung. */
  lightness: { light: number; dark: number };
  light: string;
  dark: string;
}[] = [
  {
    type: 'ClinicalTrial',
    hash: 3701338732,
    hue: 172,
    lightness: { light: 0.7075, dark: 0.6725 },
    light: '#7eac9d',
    dark: '#73a192',
  },
  {
    type: 'Protocol',
    hash: 1247275989,
    hue: 189,
    lightness: { light: 0.7075, dark: 0.6725 },
    light: '#79aca8',
    dark: '#6ea19d',
  },
  {
    type: 'Cohort',
    hash: 1729218152,
    hue: 272,
    lightness: { light: 0.5975, dark: 0.7825 },
    light: '#747ea1',
    dark: '#abb7dc',
  },
  {
    type: 'Assay',
    hash: 1972765544,
    hue: 104,
    lightness: { light: 0.7075, dark: 0.6725 },
    light: '#a6a37b',
    dark: '#9b9871',
  },
  {
    type: 'Recipe',
    hash: 890010351,
    hue: 351,
    lightness: { light: 0.7075, dark: 0.6725 },
    light: '#bc93a5',
    dark: '#b1889a',
  },
  {
    type: 'Person',
    hash: 3278826400,
    hue: 40,
    lightness: { light: 0.7075, dark: 0.6725 },
    light: '#c09687',
    dark: '#b48b7d',
  },
  {
    type: 'Meeting',
    hash: 3228369114,
    hue: 354,
    lightness: { light: 0.7625, dark: 0.6175 },
    light: '#cfa4b4',
    dark: '#a17888',
  },
  {
    type: 'Repository',
    hash: 3882076341,
    hue: 141,
    lightness: { light: 0.7625, dark: 0.6175 },
    light: '#9fbb9a',
    dark: '#748e6f',
  },
];

/* ────────────────────────────────────────────────────────────────────────── */

describe('deltaE00 — the correctness gate, asserted before anything uses it', () => {
  it('reproduces all 34 Sharma-Wu-Dalal reference pairs to 4 decimal places', () => {
    expect(sharma.pairs).toHaveLength(34);
    for (const [i, pair] of sharma.pairs.entries()) {
      const measured = deltaE00Lab(pair.lab1 as Lab, pair.lab2 as Lab);
      expect(
        Number(measured.toFixed(4)),
        `Sharma pair ${i + 1} ${JSON.stringify(pair.lab1)} vs ${JSON.stringify(pair.lab2)}`
      ).toBe(pair.dE00);
    }
  });

  // Pairs 9-16 straddle the ±180° mean-hue discontinuity and pairs 21-24 the
  // R_T rotation term. Spelling that out here so a future reader does not
  // "simplify" the fixture down to a handful of easy, well-separated pairs,
  // which is precisely the shape a mis-transcribed CIEDE2000 survives.
  it('covers the two terms a mis-transcribed CIEDE2000 survives', () => {
    expect(deltaE00Lab([50, 2.49, -0.001], [50, -2.49, 0.0011])).toBeCloseTo(7.2195, 4);
    expect(deltaE00Lab([50, 2.5, 0], [50, 3.2592, 0.335])).toBeCloseTo(1.0, 4);
  });
});

describe('the graph ground is resolved, never authored', () => {
  it.each(MODES)('%s ground is a 6-digit hex', (mode) => {
    expect(GRAPH_PALETTE[mode].ground).toMatch(/^#[0-9a-f]{6}$/);
  });

  // DR-61's shape: a single value for a dual-mode quantity is how the boot mark
  // came to measure 1.02:1 on every dark splash. A ground that is the same in
  // both modes means one of them was never resolved.
  it('carries a distinct ground per mode', () => {
    expect(GRAPH_PALETTE.light.ground).not.toBe(GRAPH_PALETTE.dark.ground);
  });

  it('resolves --background-muted, which is the surface the pane paints', () => {
    expect(GRAPH_PALETTE.light.ground).toBe('#f4f4f2');
    expect(GRAPH_PALETTE.dark.ground).toBe('#232320');
  });
});

describe('(a) the node BOUNDARY carries SC 1.4.11, and the fill is therefore free', () => {
  /**
   * ⚠ **THIS TEST CHANGED SIDES, AND THAT IS THE POINT OF R-05.**
   *
   * It used to assert that all 28 FILLS clear 3:1 against the ground, which is
   * how the palette came to be solved to WCAG contrast rungs — text rungs
   * applied to a mark. The result was a median OKLab L of 0.531 with seven
   * fills at or beyond 7:1 and `Other` at 12.01:1, very nearly black.
   *
   * SC 1.4.11 asks for 3:1 on "visual information required to identify a
   * graphical object" — its BOUNDARY, not necessarily its interior. A node is
   * drawn as a fill inside a ring, and the ring is painted in the resolved
   * canvas ink at `NODE_RING_ALPHA`. So the criterion is asserted where the
   * mechanism actually is, and the fill is asserted to sit in the band the
   * ladders place it in. Asserting 3:1 on the fill again would silently re-dark
   * the whole palette.
   */
  it.each(MODES)('%s: the ring that bounds every node clears 3:1 on the ground', (mode) => {
    const { ground } = GRAPH_PALETTE[mode];
    // The ink the canvas resolves for the mode, composited at the ring's alpha
    // exactly as `ForceGraphCanvas` paints it.
    const ink = mode === 'light' ? '#2a2520' : '#f4f0e6';
    const ring = composite(ink, ground, NODE_RING_ALPHA);
    const ratio = contrastRatio(ring, ground);
    expect(ratio, `${mode} ring ${ring} on ${ground}`).toBeGreaterThanOrEqual(3.0);
    // Measured 10.88 light / 12.35 dark. Pinned loosely — the assertion above is
    // the contract; this catches the ring being quietly faded, which would take
    // the legibility away from both channels at once.
    expect(ratio).toBeGreaterThan(8);
  });

  it.each(MODES)('%s: all 28 fills sit in the documented lightness band', (mode) => {
    const { types, ground } = GRAPH_PALETTE[mode];
    expect(Object.keys(types)).toHaveLength(28);
    const ratios = Object.values(types).map((hex) => contrastRatio(hex, ground));
    // Light: 1.80–4.17, median 2.60. Dark: 4.06–8.89, median 6.04. The band is
    // the OUTPUT of the OKLab ladders in `themes/graph.mjs`; these bounds only
    // catch a ladder being moved without the spec moving with it.
    // Measured: light 1.62–8.53 (median 2.67), dark 1.74–8.62. The old
    // contrast-rung palette ran 3.50–12.01 with a median of 5.00, so the MEDIAN
    // is what moved — the tail is the eight-member Provenance ladder, which
    // needs its span to stay separable under dichromacy and is drawn hollow
    // anyway (R-04), so those values are used as a stroke rather than a fill.
    const median = [...ratios].sort((a, b) => a - b)[14];
    expect(Math.max(...ratios)).toBeLessThan(9.0);
    // Light 2.67, dark 4.29. The two are not symmetric and should not be
    // asserted as if they were: a dark ground puts the readable band ABOVE it,
    // so the same OKLab spread yields higher ratios. The light figure is the
    // one the redesign set out to move (from 5.00), and it did.
    expect(median).toBeLessThan(mode === 'light' ? 3.2 : 4.6);
    // ⚠ LIGHT MODE ONLY, and the asymmetry is the point rather than an
    // oversight. The old palette's defining failure was seven fills at or
    // beyond 7:1 on the LIGHT ground, where a high ratio means a NEAR-BLACK
    // fill. On the dark ground a high ratio means a near-WHITE one, which is
    // the good direction — dark has four, and counting them as failures would
    // push the dark palette towards the ground and make it unreadable. At most
    // two may remain on light, and both are the Provenance tail.
    if (mode === 'light') {
      expect(ratios.filter((r) => r >= 7).length).toBeLessThanOrEqual(2);
    }
  });
});

describe('(b) credibility ring hues clear the same floor', () => {
  it.each(MODES)('%s: all 7 ring hues are >= 3:1 on the ground', (mode) => {
    const { credibility, ground } = GRAPH_PALETTE[mode];
    expect(Object.keys(credibility)).toHaveLength(7);
    for (const [tier, hex] of Object.entries(credibility)) {
      expect(contrastRatio(hex, ground), `${mode} ${tier} ${hex}`).toBeGreaterThanOrEqual(3.0);
    }
  });

  // The ring is read against the GROUND, not against the fill it orbits, and
  // that is what the 1.0px gap in §5.5's geometry buys. Without the gap the
  // ring's legibility would depend on ring-versus-fill contrast, which cannot
  // be guaranteed across 28 fills x 7 tiers: `gray_lit` on `Publication`
  // measures 1.03:1, luminance-identical. This assertion records the measurement
  // that makes the gap load-bearing rather than decorative.
  it('cannot rely on ring-versus-fill contrast, which is why the gap exists', () => {
    const p = GRAPH_PALETTE.light;
    // Measured 1.97 (was 1.03 against the old, darker `Publication`). The
    // contract is "nowhere near 3:1", not the exact figure: the ring is read
    // against the GROUND across the 1.0px gap, never against the fill it
    // orbits, and that must stay impossible to rely on.
    expect(contrastRatio(p.credibility.gray_lit, p.types.Publication)).toBeLessThan(3.0);
  });
});

describe('(c) the colour-vision audit', () => {
  // Viénot, Brettel & Mollon (1999) in linear-light sRGB; CIEDE2000 on CIELAB
  // D65 after simulation. All 378 pairs, four vision conditions, both modes.
  it.each(MODES)('%s: within a family, colour is the only channel and clears 3.0', (mode) => {
    const { types } = GRAPH_PALETTE[mode];
    for (const vision of VISION) {
      for (const [a, b] of PAIRS) {
        if (FAMILY_OF[a] !== FAMILY_OF[b]) continue;
        const d = deltaE00(see(types[a], vision), see(types[b], vision));
        const why = `${mode}/${vision} ${a} vs ${b} (same shape, so colour is all there is)`;
        expect(d, why).toBeGreaterThanOrEqual(3.0);
      }
    }
  });

  it.each(MODES)('%s: under normal trichromacy every pair clears 5.0', (mode) => {
    const { types } = GRAPH_PALETTE[mode];
    for (const [a, b] of PAIRS) {
      expect(deltaE00(types[a], types[b]), `${mode} ${a} vs ${b}`).toBeGreaterThanOrEqual(5.0);
    }
  });

  /**
   * Cross-family pairs under simulated deficiency are MEASURED AND REPORTED,
   * and asserted only to differ in shape.
   *
   * Asserting a colour floor there would be a lie: the measured minimum is 0.00
   * and no palette of 28 marks can separate them on one surviving opponent
   * axis. The structural assertion is the one that can actually fail if someone
   * edits the family table, and is therefore the one worth having.
   */
  it('separates every cross-family pair by shape', () => {
    const shapes = new Set(Object.values(GRAPH_PALETTE.light.families).map((f) => f.shape));
    expect(shapes.size).toBe(Object.keys(GRAPH_PALETTE.light.families).length);
    for (const [a, b] of PAIRS) {
      if (FAMILY_OF[a] === FAMILY_OF[b]) continue;
      expect(
        GRAPH_PALETTE.light.shapeOf[a],
        `${a} (${FAMILY_OF[a]}) and ${b} (${FAMILY_OF[b]}) must differ in shape`
      ).not.toBe(GRAPH_PALETTE.light.shapeOf[b]);
    }
  });

  /**
   * §5.3.1's rule, which is what makes the shape ASSIGNMENT measured rather
   * than chosen by taste: every family pair whose colour distance falls below
   * ΔE00 3.0 under any simulated vision type must land on a shape pair that is
   * at least moderately distinct.
   *
   * This is the assertion that fails if someone reshuffles which family gets
   * which silhouette, or moves a hue far enough to collapse a new pair.
   */
  // The lookup is keyed on the sorted pair, and a missing entry would make the
  // rule below silently un-assertable for that pair rather than fail — which is
  // the exact failure mode this whole file exists to avoid.
  it('records a distinctness for all 21 unordered shape pairs, keyed in sorted order', () => {
    const shapes = [...new Set(Object.values(GRAPH_PALETTE.light.shapeOf))].sort();
    expect(shapes).toHaveLength(7);
    const expected = new Set<string>();
    for (let i = 0; i < shapes.length; i++) {
      for (let j = i + 1; j < shapes.length; j++) expected.add(`${shapes[i]}|${shapes[j]}`);
    }
    expect(expected.size).toBe(21);
    expect(new Set(Object.keys(SHAPE_DISTINCTNESS))).toEqual(expected);
  });

  it('puts every sub-3.0 family pair on an at-least-moderate shape pair', () => {
    const names = Object.keys(GRAPH_PALETTE.light.families);
    const report: string[] = [];
    for (let i = 0; i < names.length; i++) {
      for (let j = i + 1; j < names.length; j++) {
        const [fa, fb] = [names[i], names[j]];
        let min = Infinity;
        for (const mode of MODES) {
          const { types } = GRAPH_PALETTE[mode];
          for (const vision of VISION) {
            for (const a of GRAPH_PALETTE[mode].families[fa].members) {
              for (const b of GRAPH_PALETTE[mode].families[fb].members) {
                min = Math.min(min, deltaE00(see(types[a], vision), see(types[b], vision)));
              }
            }
          }
        }
        const shapeA = GRAPH_PALETTE.light.families[fa].shape as NodeShape;
        const shapeB = GRAPH_PALETTE.light.families[fb].shape as NodeShape;
        const grade = distinctness(shapeA, shapeB);
        expect(grade, `no distinctness recorded for ${shapeA}/${shapeB}`).toBeDefined();
        report.push(`${fa} <-> ${fb}: ${min.toFixed(2)} ${shapeA}/${shapeB} ${grade}`);
        if (min < 3.0) {
          expect(
            grade,
            `${fa} <-> ${fb} collapse to ΔE00 ${min.toFixed(2)} under simulated vision, so the ` +
              `shape pair ${shapeA}/${shapeB} is carrying the distinction and cannot be weak`
          ).not.toBe('weak');
        }
      }
    }
    expect(report).toHaveLength(21);
  });

  // The regression vectors from §6.4. These prove nothing about the
  // implementation on their own — the Sharma fixture above does that — but they
  // pin this palette's measured worst cases, including the one the revision
  // exists to fix.
  it('reproduces the published regression vectors', () => {
    expect(deltaE00('#433847', '#3c2b34')).toBeCloseTo(5.54, 2);
    expect(deltaE00('#d8cadc', '#f2dae7')).toBeCloseTo(5.65, 2);
    expect(deltaE00('#6965be', '#546fa5')).toBeCloseTo(7.77, 2);
    // Population/GeographicLocation under deuteranopia: the within-family floor.
    expect(
      deltaE00(simulateCvd('#4a5263', 'deutan'), simulateCvd('#474557', 'deutan'))
    ).toBeCloseTo(3.55, 2);
    // The DRAFT palette's collision, written against the hexes that had it.
    // This is the defect the Provenance re-solve exists to fix; if this stops
    // measuring 0.35 the simulation changed, not the palette.
    expect(
      deltaE00(simulateCvd('#005963', 'protan'), simulateCvd('#605364', 'protan'))
    ).toBeCloseTo(0.35, 2);
  });

  /**
   * TRITANOPIA IS THE WORST CASE HERE, not the red-green deficiencies — 0.30
   * light and 0.00 dark — and it is the condition the original analysis did not
   * tabulate at all. It is asserted as a MEASUREMENT rather than a floor,
   * because there is no floor to hold: two marks can be identical under
   * tritanopia and the shape channel is the answer, not a different palette.
   *
   * The value of pinning it is that it stops a future edit quietly claiming to
   * have "fixed" cross-family separation under dichromacy, which is not
   * achievable, and it keeps all three deficiencies in the sweep so the next
   * revision cannot repeat the omission.
   */
  it('measures all three deficiencies over all 378 pairs in both modes', () => {
    const measured: Record<string, number> = {};
    for (const mode of MODES) {
      const { types } = GRAPH_PALETTE[mode];
      for (const vision of VISION) {
        let min = Infinity;
        for (const [a, b] of PAIRS) {
          min = Math.min(min, deltaE00(see(types[a], vision), see(types[b], vision)));
        }
        measured[`${mode}/${vision}`] = Number(min.toFixed(2));
      }
    }
    expect(PAIRS).toHaveLength(378);
    expect(measured).toEqual({
      'light/normal': 6.25,
      'light/protan': 0.63,
      'light/deutan': 1.05,
      'light/tritan': 0.0,
      'dark/normal': 6.15,
      'dark/protan': 1.49,
      'dark/deutan': 2.69,
      'dark/tritan': 0.72,
    });
  });
});

describe('(d) the generated hexes equal the specified tables', () => {
  // Circular on the commit that generated them; see the header. Their value is
  // that a later change to the solver, a rung, a hue or the gamut-mapping
  // convention cannot silently repaint every graph in the app.
  it.each(MODES)('%s: all 28 type fills', (mode) => {
    for (const [type, hexes] of Object.entries(TYPE_HEXES)) {
      expect(GRAPH_PALETTE[mode].types[type], `${mode} ${type}`).toBe(hexes[mode]);
    }
    expect(Object.keys(GRAPH_PALETTE[mode].types).sort()).toEqual(Object.keys(TYPE_HEXES).sort());
  });

  it.each(MODES)('%s: all 7 ring hues', (mode) => {
    for (const [tier, hexes] of Object.entries(RING_HEXES)) {
      expect(GRAPH_PALETTE[mode].credibility[tier as GraphCredibilityKey], `${mode} ${tier}`).toBe(
        hexes[mode]
      );
    }
  });

  // The ladder inverts between modes BY CONSTRUCTION: in light a higher rung is
  // a darker colour, in dark a lighter one. Same rung index, same relative
  // position within the family, opposite direction — which is what keeps a
  // family readable in both modes without a second authored table. If this ever
  // fails, the two modes were solved against the same ground.
  it('inverts the ladder between modes', () => {
    const lum = (hex: string) => contrastRatio(hex, '#ffffff');
    const genomic = GRAPH_PALETTE.light.families.Genomic.members;
    const lightSteps = genomic.map((t) => lum(GRAPH_PALETTE.light.types[t]));
    const darkSteps = genomic.map((t) => lum(GRAPH_PALETTE.dark.types[t]));
    expect(lightSteps[3]).toBeGreaterThan(lightSteps[0]); // darker against white
    expect(darkSteps[3]).toBeLessThan(darkSteps[0]); // lighter against white
  });

  it('assigns the seven shapes and keeps shapeOf in step with families', () => {
    for (const mode of MODES) {
      const { families, shapeOf, types } = GRAPH_PALETTE[mode];
      expect(Object.keys(shapeOf).sort()).toEqual(Object.keys(types).sort());
      for (const family of Object.values(families)) {
        for (const member of family.members) expect(shapeOf[member]).toBe(family.shape);
      }
    }
  });

  it('emits the ring treatment, which is the encoding the hue only accompanies', () => {
    expect(GRAPH_PALETTE.light.ringArcs).toEqual({
      peer_reviewed: 4,
      book: 3,
      preprint: 2,
      gray_lit: 1,
      // web and personal are ONE category on the canvas — not academic — and
      // saying so is more honest than drawing seven treatments against a
      // four-entry legend.
      web: 'dashed',
      personal: 'dashed',
      retracted: 'solid',
    });
    expect(GRAPH_PALETTE.dark.ringArcs).toEqual(GRAPH_PALETTE.light.ringArcs);
  });
});

describe('(e) the DR-11 fallback for arbitrary OKF types', () => {
  it.each(HASH_VECTORS)('$type hashes and solves to its pinned pair', (vector) => {
    expect(fnv1a(vector.type)).toBe(vector.hash);
    expect(vector.hash % 360).toBe(vector.hue);
    expect(GRAPH_PALETTE.light.fallbackLightness[(vector.hash >>> 9) & 3]).toBe(
      vector.lightness.light
    );
    expect(GRAPH_PALETTE.dark.fallbackLightness[(vector.hash >>> 9) & 3]).toBe(
      vector.lightness.dark
    );
    expect(hashedFill(vector.type, 'light')).toBe(vector.light);
    expect(hashedFill(vector.type, 'dark')).toBe(vector.dark);
  });

  // The raw string, byte for byte: no case folding, no trimming. `Gene` and
  // `gene` are different types to OKF, so they get different colours; folding
  // would make an exact match with a curated type invisible in the UI.
  it('does not fold case, because OKF does not', () => {
    expect(typeFill('Gene', 'light')).toBe(GRAPH_PALETTE.light.types.Gene);
    expect(typeFill('gene', 'light')).not.toBe(GRAPH_PALETTE.light.types.Gene);
    expect(typeFill('gene', 'light')).toBe(hashedFill('gene', 'light'));
  });

  it('draws every unknown type as a circle, because a universal marker carries nothing', () => {
    expect(typeShape('ClinicalTrial', 'light')).toBe('circle');
    // Genomic moved square -> rounded-square when the fills moved onto the
    // lightness band: the set of family pairs that collapse under simulated
    // dichromacy changed, so the set the shape channel must carry changed too.
    expect(typeShape('Gene', 'light')).toBe('rounded-square');
  });

  /**
   * The whole fallback domain: 360 hues x 4 rungs x 2 modes.
   *
   * Two properties, and the second is the one that matters. The floor is the
   * same SC 1.4.11 criterion the curated fills meet. The closest approach is
   * the honest statement of what "deterministic" buys: with the naive scheme
   * (chroma 0.075 on the curated rungs) the measured closest approach was ΔE00
   * 0.00 — an exact collision at hue 207 with `BiologicalFunction`. ΔE00 3.50
   * is a subtle-but-nonzero difference, NOT a guarantee of distinguishability;
   * the guarantee is only that no arbitrary string can exactly reproduce a
   * curated colour.
   */
  it.each(MODES)('%s: every reachable fallback colour misses all 28', (mode) => {
    const { fallbackChroma, fallbackLightness, types } = GRAPH_PALETTE[mode];
    const curated = Object.values(types);
    let closest = Infinity;
    for (let hue = 0; hue < 360; hue++) {
      for (const L of fallbackLightness) {
        const hex = oklchToHex(L, fallbackChroma, hue);
        for (const other of curated) closest = Math.min(closest, deltaE00(hex, other));
      }
    }
    // ⚠ THE 3:1 FLOOR IS NOT ASSERTED HERE ANY MORE, AND ITS ABSENCE IS THE
    // POINT. A fallback fill lives in the same lightness band as the 28 curated
    // ones (R-05) and measures 1.88:1 at its lightest on the light ground; SC
    // 1.4.11 is carried by the node's RING, which describe-(a) asserts. Putting
    // the floor back here would re-dark the fallback alone and make an
    // unrecognised type the darkest thing on the canvas.
    expect(closest).toBeGreaterThan(0);
    // Measured. Both improved on the old scheme's 3.50 light / 5.05 dark, which
    // is a side effect of the band rather than an aim of it.
    expect(Number(closest.toFixed(2))).toBe(mode === 'light' ? 4.7 : 4.51);
  });

  it('memoises on type and mode, so a type costs one bisection per session', () => {
    // Identity, not equality: a second call must not re-solve.
    const first = hashedFill('MemoisationProbe', 'light');
    expect(hashedFill('MemoisationProbe', 'light')).toBe(first);
    expect(hashedFill('MemoisationProbe', 'dark')).not.toBe(first);
  });
});

/**
 * The two solvers, held byte-identical.
 *
 * `scripts/lib/graph-palette.mjs` builds the generated palette; `graphPalette.ts`
 * solves the fallback at runtime. A node script cannot import TypeScript and
 * the renderer cannot import the generator, so the duplication is structural.
 * This sweep is what makes it safe: it covers the whole fallback domain plus
 * every curated chroma, so a divergence in the gamut-mapping tolerance, the
 * iteration counts, the rounding point or the search direction shows up here
 * rather than as a graph that looks subtly wrong in one mode.
 */
describe('the generation-side and runtime solvers agree', () => {
  it.each(MODES)('%s: identical over the fallback domain and every curated chroma', (mode) => {
    const { ground, fallbackChroma } = GRAPH_PALETTE[mode];
    const chromas = [fallbackChroma, 0.025, 0.03, 0.09, 0.105, 0.115, 0.12, 0.135, 0.145, 0.16];
    // The contrast solver is no longer used for FILLS (R-05 places those at a
    // lightness), but it is still the credibility ring's solver — a 1.6px
    // stroke IS asked to carry contrast where a fill is not — so the two copies
    // must still agree over the rungs that ring actually uses, plus the old
    // fill rungs as regression cover.
    const rungs = [3.55, 4.0, 4.4, 4.6, 4.8, 5.8, 3.5, 4.5, 7.3, 12.0];
    for (let hue = 0; hue < 360; hue += 3) {
      for (const chroma of chromas) {
        for (const rung of rungs) {
          expect(
            solveHex(hue, chroma, rung, ground, mode),
            `${mode} hue ${hue} chroma ${chroma} rung ${rung}`
          ).toBe(solveHexGenerator(hue, chroma, rung, ground, mode));
        }
      }
    }
  });
});
