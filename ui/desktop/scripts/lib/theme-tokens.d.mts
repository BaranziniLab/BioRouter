/**
 * Types for `theme-tokens.mjs`, so the ONE implementation of the colour maths
 * can be asserted from a Vitest test as well as used by the node guards.
 *
 * `graphPalette.test.ts` needs `deltaE00` and `simulateCvd`, and duplicating
 * either in TypeScript would defeat the reason that module exists — the
 * previous generation of guards each reimplemented the WCAG maths, and one of
 * them came to verify a syntax palette against a surface the app never painted.
 * A declaration file is the cheapest way to have one implementation and two
 * consumers.
 *
 * Only the exports a TypeScript consumer actually needs are declared. Adding an
 * export to the .mjs does not require a line here until something typed imports
 * it.
 */

/** A CIELAB triple: L*, a*, b* (D65, 2°). */
export type Lab = [number, number, number];

/** The three dichromacies the palette guard simulates. */
export type Dichromacy = 'protan' | 'deutan' | 'tritan';

export function luminance(hex: string): number;
export function contrast(a: string, b: string): number;
export function blend(fillHex: string, alpha: number, groundHex: string): string;

export function hexToLab(hex: string): Lab;
export function deltaE00Lab(labA: Lab, labB: Lab): number;
export function deltaE00(a: string, b: string): number;
export function simulateCvd(hex: string, kind: Dichromacy): string;
