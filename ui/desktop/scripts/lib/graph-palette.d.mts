/**
 * Types for `graph-palette.mjs` — the generation-side copy of the §5.2 solver.
 *
 * Declared so `graphPalette.test.ts` can sweep this implementation against the
 * renderer's copy in `src/styles/graphPalette.ts` and assert them
 * byte-identical. Two solvers exist because a node script cannot import
 * TypeScript and the renderer cannot import the generator; that sweep is what
 * makes the duplication safe rather than merely tolerated.
 */

export function oklchToHex(L: number, chroma: number, hueDeg: number): string;

export function solveHex(
  hue: number,
  chroma: number,
  rung: number,
  ground: string,
  mode: 'light' | 'dark'
): string;

export function memberHue(anchorHue: number, spread: number, i: number, n: number): number;
