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
 * The band below is the result. Measured against the light ground, the 28
 * fills run 1.62–9.52:1 with a MEDIAN of 2.39:1, against the old palette's
 * 3.50–12.01:1 and median 5.00:1 — and zero fills at or beyond 7:1 among the
 * twenty biological types.
 *
 * ⚠ WITHIN-FAMILY SEPARATION SURVIVES BY CONSTRUCTION, and that is measured
 * rather than hoped for. The step between adjacent members is 0.055 L against
 * the old ladder's 0.050–0.059, and the hues, chromas and spreads are
 * untouched — so the CIEDE2000 minimum across all seven families moves from
 * 5.54 to 4.80, against a guard floor of 3.0. `graphPalette.test.ts` re-measures
 * it; do not take this comment as the guarantee.
 *
 * ⚠ **THE LIGHT END IS CAPPED AT 0.78, AND THE CAP IS THE HARD-WON PART.** A
 * lighter band is not free: above ~0.80 the sRGB gamut clips chroma sharply, so
 * every family's lightest member desaturates towards the same pale tint and
 * CROSS-family pairs start collapsing under simulated dichromacy. Measured over
 * the 21 family pairs:
 *
 *     old contrast-rung palette   7 of 21 pairs below ΔE00 3.0
 *     a 0.80–0.50 / 0.84–0.48 band   13 of 21   <- and NO assignment of the
 *                                                  seven shapes can cover 13
 *     this band (0.78 cap)          11 of 21   <- coverable, and covered
 *
 * Cross-family separation was never carried by colour — it bottoms out at ΔE00
 * 0.24 here and 0.00 in the old palette — it is carried by the SHAPE channel,
 * and the shape channel only works if every sub-3.0 pair lands on a shape pair
 * that is not "weak". A parametric sweep over both spans found 36 bands where
 * such an assignment exists; this is the lightest of them. Lightening past 0.78
 * breaks the guard in `graphPalette.test.ts` that checks exactly this, and the
 * right response to that failure is to come back here, not to relax the guard.
 *
 * DARK IS SOLVED SEPARATELY AND ASCENDS. On a dark ground the readable band
 * sits above the ground rather than below it, so member 0 is the DARKEST and
 * the ladder climbs. It is not the light ladder inverted, and it must be
 * re-audited on its own.
 */
export const PRIMARY_L = {
  light: [0.78, 0.6933, 0.6067, 0.52],
  dark: [0.52, 0.6067, 0.6933, 0.78],
};

/**
 * The eight-member Provenance ladder — the WIDEST span in the palette, and the
 * reason is arithmetic rather than taste.
 *
 * ⚠ **THIS SPAN IS A MEASURED FLOOR, NOT A PREFERENCE.** At chroma 0.030 —
 * where dichromacy leaves lightness as very nearly the only surviving channel —
 * eight members need roughly 0.36 of OKLab L between them to hold ΔE00 3.0
 * apart under simulated protanopia, deuteranopia and tritanopia. A first draft
 * of this redesign used 0.780–0.584 to keep the whole palette light, and it
 * measured 2.11 (dark/protan, `Population`/`GeographicLocation`) and 2.71
 * (light/tritan, `Dataset`/`Agent`) — both below the floor the guard asserts.
 * A parametric sweep over span and chroma found no combination that keeps eight
 * members separable in a narrow band; raising chroma to 0.045 bought only 0.5
 * ΔE00 and collided with the fallback's own chroma argument. The span stays.
 *
 * ⚠ **This is the one family whose darker tail costs nothing**, which is what
 * makes the trade acceptable rather than merely necessary: R-04 draws
 * Provenance & Context as HOLLOW rings, so these values are used as a STROKE
 * rather than as a fill — and a stroke is exactly where more contrast helps.
 * The twenty biological types, which are what a reader actually looks at, stay
 * in the light band.
 *
 * Measured within-family minimum across all four vision types: ΔE00 4.28.
 */
export const PROVENANCE_L = {
  light: [0.78, 0.7257, 0.6714, 0.6171, 0.5629, 0.5086, 0.4543, 0.4],
  dark: [0.4, 0.4543, 0.5086, 0.5629, 0.6171, 0.6714, 0.7257, 0.78],
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
 * ⚠ THE SHAPE ASSIGNMENT MOVED WITH THE PALETTE, AND HAD TO. Four families
 * swapped silhouettes when the fills moved onto the lightness band (Genomic
 * square→rounded-square, Clinical rounded-square→square, Exposome
 * pentagon→circle, Physical circle→pentagon). That is not churn: the set of
 * family pairs that collapse under simulated dichromacy changed, so the set of
 * pairs the shape channel has to carry changed with it. The assignment below is
 * the minimal-change permutation that leaves no collapsed pair on a "weak"
 * shape pair.
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
    shape: 'rounded-square',
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
    shape: 'square',
    anchorHue: 18,
    chroma: 0.145,
    spread: 34,
    ladder: 'primary',
    members: ['Disease', 'Phenotype', 'BiomedicalMeasure', 'MethodOrProcedure'],
  },
  {
    name: 'Exposome',
    shape: 'circle',
    anchorHue: 78,
    chroma: 0.12,
    spread: 24,
    ladder: 'primary',
    members: ['Exposure', 'SocialFactor', 'Food'],
  },
  {
    name: 'Physical',
    shape: 'pentagon',
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
