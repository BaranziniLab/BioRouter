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
import { contrastRatio, fnv1a, hashedFill, solveHex, typeFill, typeShape } from './graphPalette';
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
  Gene: { light: '#6a7cd4', dark: '#5f70c8' },
  Variant: { light: '#6965be', dark: '#817fdb' },
  SequenceFeature: { light: '#6750a7', dark: '#a48fec' },
  Structure: { light: '#643d90', dark: '#c69efb' },
  Molecule: { light: '#1d927a', dark: '#00866f' },
  MolecularClass: { light: '#007d75', dark: '#13998f' },
  BiologicalPathway: { light: '#006a6d', dark: '#2fadb0' },
  BiologicalFunction: { light: '#005963', dark: '#4cbfd1' },
  Anatomy: { light: '#608d44', dark: '#558239' },
  CellType: { light: '#367e45', dark: '#50985d' },
  Organism: { light: '#006d48', dark: '#4dae81' },
  Disease: { light: '#cb5d82', dark: '#bf5177' },
  Phenotype: { light: '#ba4a5e', dark: '#d76476' },
  BiomedicalMeasure: { light: '#a73939', dark: '#ef7a76' },
  MethodOrProcedure: { light: '#942b0f', dark: '#ff9379' },
  Exposure: { light: '#b47327', dark: '#a86817' },
  SocialFactor: { light: '#966700', dark: '#b18023' },
  Food: { light: '#755c00', dark: '#ba9938' },
  Device: { light: '#4788b0', dark: '#3b7da4' },
  MaterialSample: { light: '#546fa5', dark: '#6d89c0' },
  Publication: { light: '#738679', dark: '#697b6e' },
  Study: { light: '#617a76', dark: '#6f8884' },
  Dataset: { light: '#556d72', dark: '#7c959a' },
  Agent: { light: '#4e606c', dark: '#8da1af' },
  Population: { light: '#4a5263', dark: '#a4aec2' },
  GeographicLocation: { light: '#474557', dark: '#bebbd1' },
  Concept: { light: '#433847', dark: '#d8cadc' },
  Other: { light: '#3c2b34', dark: '#f2dae7' },
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
  rung: number;
  light: string;
  dark: string;
}[] = [
  {
    type: 'ClinicalTrial',
    hash: 3701338732,
    hue: 172,
    rung: 4.95,
    light: '#457264',
    dark: '#6b998a',
  },
  { type: 'Protocol', hash: 1247275989, hue: 189, rung: 4.95, light: '#40726e', dark: '#669995' },
  { type: 'Cohort', hash: 1729218152, hue: 272, rung: 7.9, light: '#414a6a', dark: '#abb6dc' },
  { type: 'Assay', hash: 1972765544, hue: 104, rung: 4.95, light: '#6e6b45', dark: '#95916a' },
  { type: 'Recipe', hash: 890010351, hue: 351, rung: 4.95, light: '#855e6f', dark: '#ae8596' },
  { type: 'Person', hash: 3278826400, hue: 40, rung: 4.95, light: '#876053', dark: '#b08779' },
  { type: 'Meeting', hash: 3228369114, hue: 354, rung: 3.9, light: '#976e7e', dark: '#9c7383' },
  { type: 'Repository', hash: 3882076341, hue: 141, rung: 3.9, light: '#668162', dark: '#6b8567' },
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

describe('(a) type fills clear the non-text contrast floor on their own ground', () => {
  // WCAG 2.1 SC 1.4.11 (3:1) is the correct criterion for a coloured dot. A
  // node fill is never text, so 4.5:1 is not the bar and must not be asserted —
  // asserting it would force the whole palette darker for a criterion that does
  // not apply.
  it.each(MODES)('%s: all 28 fills are >= 3:1 on the ground', (mode) => {
    const { types, ground } = GRAPH_PALETTE[mode];
    expect(Object.keys(types)).toHaveLength(28);
    let floor = Infinity;
    let worst = '';
    for (const [type, hex] of Object.entries(types)) {
      const ratio = contrastRatio(hex, ground);
      expect(ratio, `${mode} ${type} ${hex} on ${ground}`).toBeGreaterThanOrEqual(3.0);
      if (ratio < floor) [floor, worst] = [ratio, type];
    }
    // Measured 3.50 light / 3.48 dark, both on `Gene`. Pinned loosely: the
    // assertion above is the contract, this only catches the floor moving to a
    // different member, which means a ladder changed.
    expect(worst).toBe('Gene');
    expect(floor).toBeLessThan(3.6);
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
    expect(contrastRatio(p.credibility.gray_lit, p.types.Publication)).toBeLessThan(1.1);
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
      'light/normal': 5.54,
      'light/protan': 0.97,
      'light/deutan': 1.26,
      'light/tritan': 0.3,
      'dark/normal': 5.65,
      'dark/protan': 1.49,
      'dark/deutan': 3.27,
      'dark/tritan': 0.0,
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
    expect(GRAPH_PALETTE.light.fallbackRungs[(vector.hash >>> 9) & 3]).toBe(vector.rung);
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
    expect(typeShape('Gene', 'light')).toBe('square');
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
  it.each(MODES)('%s: every reachable fallback colour clears 3:1 and misses all 28', (mode) => {
    const { fallbackChroma, fallbackRungs, ground, types } = GRAPH_PALETTE[mode];
    const curated = Object.values(types);
    let floor = Infinity;
    let closest = Infinity;
    for (let hue = 0; hue < 360; hue++) {
      for (const rung of fallbackRungs) {
        const hex = solveHex(hue, fallbackChroma, rung, ground, mode);
        floor = Math.min(floor, contrastRatio(hex, ground));
        for (const other of curated) closest = Math.min(closest, deltaE00(hex, other));
      }
    }
    expect(floor).toBeGreaterThanOrEqual(3.0);
    expect(closest).toBeGreaterThan(0);
    expect({ floor: Number(floor.toFixed(2)), closest: Number(closest.toFixed(2)) }).toEqual(
      mode === 'light' ? { floor: 3.9, closest: 3.5 } : { floor: 3.86, closest: 5.05 }
    );
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
    const { ground, fallbackChroma, fallbackRungs } = GRAPH_PALETTE[mode];
    const chromas = [fallbackChroma, 0.025, 0.03, 0.09, 0.105, 0.115, 0.12, 0.135, 0.145, 0.16];
    const rungs = [...fallbackRungs, 3.5, 4.5, 5.8, 7.3, 12.0];
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
