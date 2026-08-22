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
 * THE FILL LADDERS ARE LIGHTNESS, NOT CONTRAST (redesign R-05).
 *
 * ⚠ THIS REPLACED A CONTRAST-RUNG LADDER, AND THE REASON IS THE WHOLE FIX. The
 * fills used to be solved to WCAG ratios against the ground — rungs of 3.5,
 * 4.5, 5.8 and 7.3, with a Provenance ladder running to 12.0:1. Those are TEXT
 * rungs applied to a MARK. Measured against the light ground, they put the 28
 * fills at a median OKLab L of 0.531 with seven of them at or beyond 7:1, and
 * `Other` at 12.01:1 — very nearly black. The canvas read as a field of dark
 * dots because the FILL was being asked to carry contrast that belongs to the
 * node's boundary.
 *
 * BioOKF's graph is lighter for a measurable reason: it puts a near-black
 * hairline around every circle (17.68:1 against its ground) and lets the fill
 * be a light mid-tone (median L 0.648). This palette does the same. The node
 * ring moved from alpha 0.50 to 0.85 — composited, 9.02:1 against the light
 * ground — which is what satisfies WCAG 1.4.11 for the graphical object's
 * boundary and therefore frees the fill.
 *
 * The band below is the result: median L 0.690, range 0.580–0.780.
 *
 * ⚠ WITHIN-FAMILY SEPARATION SURVIVES BY CONSTRUCTION, and that is measured
 * rather than hoped for. The step between adjacent members is 0.055 L against
 * the old ladder's 0.050–0.059, and the hues, chromas and spreads are
 * untouched — so the CIEDE2000 minimum across all seven families moves from
 * 5.54 to 4.80, against a guard floor of 3.0. `graphPalette.test.ts` re-measures
 * it; do not take this comment as the guarantee.
 *
 * ⚠ CROSS-family separation under dichromacy is NOT repaired by this and never
 * was: it bottoms out at ΔE00 0.00 with 28 marks on one surviving opponent
 * axis. That is what the shape channel existed for, and R-04's all-circle
 * canvas is an explicit, recorded trade — see the redesign spec.
 *
 * DARK IS SOLVED SEPARATELY AND ASCENDS. On a dark ground the readable band
 * sits above the ground rather than below it, so member 0 is the DARKEST and
 * the ladder climbs. It is not the light ladder inverted, and it must be
 * re-audited on its own.
 */
export const PRIMARY_L = {
  light: [0.745, 0.69, 0.635, 0.58],
  dark: [0.64, 0.695, 0.75, 0.805],
};

/**
 * The eight-member Provenance ladder, at a tighter step because it has twice
 * the members in a comparable band.
 *
 * The old ladder's accessibility fix is preserved in kind: members sit at even
 * lightness intervals at chroma 0.030, where dichromacy leaves lightness as the
 * only surviving channel, so an even ladder is what keeps them apart at all.
 * Measured within-family minimum: ΔE00 4.80 light.
 */
export const PROVENANCE_L = {
  light: [0.78, 0.752, 0.724, 0.696, 0.668, 0.64, 0.612, 0.584],
  dark: [0.6, 0.629, 0.657, 0.686, 0.714, 0.743, 0.771, 0.8],
};

/**
 * The seven families: an anchor hue, a chroma, a hue spread, a SHAPE, and the
 * member types in ladder order.
 *
 * ⚠ `shape` IS STILL DECLARED, AND IS NOT DEAD. The default canvas draws every
 * node as a circle (R-04), but the seven silhouettes remain the payload of the
 * `Distinguish types by shape` accessibility preference — which is what makes
 * the all-circle default a reversible trade rather than a deletion. Removing
 * `shape` here removes the user's way back.
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
/**
 * The fallback's own lightness ladder, offset a half-step from the curated one
 * so a hashed colour cannot land exactly on a curated fill's lightness. Do NOT
 * read that offset as a guarantee of separation — the guarantee is the measured
 * ΔE00 floor in `graphPalette.test.ts`, which re-measures after any change
 * here, and the chroma gap (0.055 against Provenance's 0.030).
 */
export const FALLBACK_L = {
  light: [0.7625, 0.7075, 0.6525, 0.5975],
  dark: [0.6175, 0.6725, 0.7275, 0.7825],
};

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
