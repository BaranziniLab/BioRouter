/**
 * The knowledge-graph node palette — the NUMBERS, not the colours.
 *
 * ui-spec.md §5.2/§5.3/§5.4/§5.5 and design.md DR-10/DR-11. Everything here is
 * an input to the derivation in `scripts/lib/graph-palette.mjs`; the 28 type
 * fills, the 7 credibility ring hues and the hashed fallback are SOLVED from
 * these against the graph ground and emitted into
 * `src/styles/themes.generated.ts`.
 *
 * TWO THINGS THIS FILE DELIBERATELY DOES NOT HOLD:
 *
 *   - No hex value. A hex here would be a second source of truth for a colour
 *     the solver already determines, and the tables in §5.3 are the solver's
 *     pinned OUTPUT, not its input.
 *   - No ground. The ground is `--background-muted`, RESOLVED out of the
 *     stylesheet by the generator in all six (family × mode) scopes — the same
 *     category CLAUDE.md lists under "Derived, never authored: terminal/code/
 *     splash grounds… These are the values that historically drifted." Pinning
 *     `{ light: '#f4f4f2', dark: '#232320' }` here is exactly the shape of
 *     DR-61, where a single light value for a dual-mode quantity left the boot
 *     mark at 1.02:1 on every dark splash.
 *
 * It sits beside `themes/*.theme.mjs` but is NOT a theme: it has no family id,
 * `generate-themes.mjs` never sweeps it up with the `.theme.mjs` glob, and it
 * adds ZERO theme tokens. One palette serves all three families precisely
 * because they share `--background-muted`, and that sharing is asserted rather
 * than assumed (see the six-scope identity check in the generator).
 */

/**
 * The contrast rung ladder for families of 2–4 members, by member index.
 *
 * A rung is a WCAG 2.x contrast ratio against the graph ground. The solver
 * finds the OKLab lightness that hits it, so "how dark" is a derived property
 * instead of an accident — which is the whole reason BioOKF Studio's 28
 * hand-picked hexes were not reused (nine of them fall below 3:1 even on the
 * near-white ground they were picked against).
 */
export const PRIMARY_RUNGS = [3.5, 4.5, 5.8, 7.3];

/**
 * The eight-member Provenance ladder — monotone and geometric, spanning the
 * same 3.50 floor to a 12.00 ceiling that still sits below the canvas label
 * ink (13.4–15.1:1).
 *
 * ⚠ AUTHORED AT TWO DECIMALS ON PURPOSE, not recomputed from
 * `3.50 * (12.00 / 3.50) ** (i / 7)`. The full-precision values differ from
 * these in the third decimal (i=3 is 5.93483, i=6 is 10.06349), and because the
 * solver rounds to 8 bits before taking the ratio, that difference moves two
 * rungs by one 8-bit step: `Agent` solves to #4d606c instead of #4e606c and
 * `Concept` to #433747 instead of #433847. Measured, not theorised — the
 * remaining 26 fills are identical either way. §5.3's table is the pinned
 * output of THESE numbers.
 *
 * This ladder is the accessibility fix of the revision, and the only palette
 * parameter that moved: the draft interleaved two four-rung ladders here, which
 * at chroma 0.030 (where dichromacy leaves lightness as the only surviving
 * channel) put members at neighbouring lightness with no hue left to separate
 * them — a within-family minimum of ΔE00 1.82. This ladder measures 3.55.
 */
export const PROVENANCE_RUNGS = [3.5, 4.17, 4.98, 5.93, 7.08, 8.44, 10.06, 12.0];

/**
 * The seven families: an anchor hue, a chroma, a hue spread, a SHAPE, and the
 * member types in ladder order.
 *
 * Working space is OKLCH — perceptually uniform, so a fixed chroma reads as an
 * equal amount of colour across hues and a spread reads as an equal amount of
 * rotation. Member i takes `H0 + (i / (n - 1) - 0.5) * spread` and rung i of
 * the family's ladder.
 *
 * SHAPE CARRIES FAMILY; LIGHTNESS CARRIES THE MEMBER. That is the redundant
 * non-colour channel WCAG 1.4.1 requires, and it is why the palette can be
 * honest about cross-family colour distance under dichromacy (measured minimum
 * 0.00 — no palette of 28 marks can do better on one surviving opponent axis).
 *
 * ⚠ THE SHAPE ASSIGNMENT IS MEASURED, NOT CHOSEN BY TASTE. Every family pair
 * whose colour distance falls below ΔE00 3.0 under any simulated vision type
 * lands on a shape pair that is at least moderately distinct, and all four
 * mutually-confusable "round-ish" pairings ({circle, hexagon, pentagon,
 * rounded-square}) are pushed onto family pairs that are ≥ 6.84 apart in
 * colour. Reshuffling `shape` below without re-running the audit in
 * `src/styles/graphPalette.test.ts` breaks that, and the audit is what will
 * tell you.
 */
export const FAMILIES = [
  {
    name: 'Genomic',
    shape: 'square',
    anchorHue: 288,
    chroma: 0.135,
    spread: 30,
    ladder: 'primary',
    members: ['Gene', 'Variant', 'SequenceFeature', 'Structure'],
  },
  {
    name: 'Molecular & process',
    shape: 'diamond',
    anchorHue: 192,
    chroma: 0.105,
    spread: 34,
    ladder: 'primary',
    members: ['Molecule', 'MolecularClass', 'BiologicalPathway', 'BiologicalFunction'],
  },
  {
    name: 'Anatomy & organism',
    shape: 'triangle',
    anchorHue: 148,
    chroma: 0.115,
    spread: 26,
    ladder: 'primary',
    members: ['Anatomy', 'CellType', 'Organism'],
  },
  {
    name: 'Clinical',
    shape: 'rounded-square',
    anchorHue: 18,
    chroma: 0.145,
    spread: 34,
    ladder: 'primary',
    members: ['Disease', 'Phenotype', 'BiomedicalMeasure', 'MethodOrProcedure'],
  },
  {
    name: 'Exposome',
    shape: 'pentagon',
    anchorHue: 78,
    chroma: 0.12,
    spread: 24,
    ladder: 'primary',
    members: ['Exposure', 'SocialFactor', 'Food'],
  },
  {
    name: 'Physical',
    shape: 'circle',
    anchorHue: 250,
    chroma: 0.09,
    spread: 26,
    ladder: 'primary',
    members: ['Device', 'MaterialSample'],
  },
  {
    name: 'Provenance & context',
    shape: 'hexagon',
    anchorHue: 250,
    chroma: 0.03,
    spread: 190,
    ladder: 'provenance',
    members: [
      'Publication',
      'Study',
      'Dataset',
      'Agent',
      'Population',
      'GeographicLocation',
      'Concept',
      'Other',
    ],
  },
];

/**
 * The DR-11 fallback for arbitrary OKF types, which in OKF mode is essentially
 * every node — so it must look native, not like an error state.
 *
 * Chroma sits deliberately between Provenance (0.030) and every biological
 * family (0.090–0.145): an unrecognised type reads as quieter than a curated
 * biological family and more coloured than provenance. An honest signal, at no
 * cost, that the vocabulary did not recognise the string.
 *
 * The rungs are the fallback's own, offset from the curated ladders. Do NOT
 * take that offset as a guarantee of separation — 4.95 sits 0.03 from
 * Provenance's 4.98, so two colours CAN coincide in lightness. The separation
 * there is carried by chroma (0.055 against 0.030) and it is MEASURED: the
 * closest a hashed colour comes to any Provenance member is ΔE00 5.19 light /
 * 5.06 dark. The guarantee is the measured floor and the guard that re-measures
 * it, never the rung arithmetic.
 */
export const FALLBACK_CHROMA = 0.055;
export const FALLBACK_RUNGS = [3.9, 4.95, 6.2, 7.9];

/**
 * The credibility ring — DR-9b. Seven rows, hue and chroma stated so the ring
 * is regenerable by the same solver as the fills.
 *
 * ⚠ `arcs` IS THE ENCODING; the hue is a bonus for trichromatic vision and
 * carries nothing alone. A 1.6px stroke subtends 2.2–2.9 arcmin at reading
 * distance, well inside the regime where the visual system reads luminance
 * only, and the seven hues measure ΔE00 1.13 apart under light/tritanopia.
 * Counting is not a colour judgement, so the count survives 2 arcmin,
 * monochrome and every simulated deficiency.
 *
 * `web` and `personal` share the dashed treatment because they ARE one category
 * on the canvas — "not academic". Saying so is more honest than drawing seven
 * treatments against a four-entry legend, which is what the draft did.
 *
 * The four academic tiers share hue 250 and fade from saturated blue to neutral
 * grey, which reads as *how much blue is left*; `web` (amber) and `personal`
 * (rose) step off the ramp because they are a different KIND of source.
 */
export const CREDIBILITY = [
  { tier: 'peer_reviewed', hue: 250, chroma: 0.12, rung: 5.8, arcs: 4 },
  { tier: 'book', hue: 250, chroma: 0.09, rung: 4.8, arcs: 3 },
  { tier: 'preprint', hue: 250, chroma: 0.06, rung: 4.0, arcs: 2 },
  { tier: 'gray_lit', hue: 250, chroma: 0.025, rung: 3.55, arcs: 1 },
  { tier: 'web', hue: 60, chroma: 0.11, rung: 4.4, arcs: 'dashed' },
  { tier: 'personal', hue: 345, chroma: 0.1, rung: 4.4, arcs: 'dashed' },
  { tier: 'retracted', hue: 25, chroma: 0.16, rung: 4.6, arcs: 'solid' },
];

/** Every shape a node can take. The order is the legend's order. */
export const NODE_SHAPES = [
  'circle',
  'square',
  'rounded-square',
  'diamond',
  'triangle',
  'pentagon',
  'hexagon',
];
