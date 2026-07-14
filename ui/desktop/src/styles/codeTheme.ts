/**
 * The BioRouter syntax palette (design.md §5.1, decision D-10).
 *
 * Derived from the warm neutral ramp rather than imported from a stock theme, so
 * code sits on the same ground as the rest of the app instead of reading as a
 * pasted-in foreign object. Every foreground below clears WCAG AA (4.5:1)
 * against its stated background; the ratios are asserted in codeTheme.test.ts.
 *
 * This is the ONLY place a syntax colour may be defined. Both the chat markdown
 * renderer and the artifact preview import from here.
 */
import type { CSSProperties } from 'react';

type PrismTheme = Record<string, CSSProperties>;

/** Ground each palette is measured against. Matches --background-muted. */
export const CODE_BG = { light: '#faf8f3', dark: '#16120c' } as const;

/** Shared with the xterm terminal so a pasted command and its output match. */
export const CODE_FONT_FAMILY = 'var(--font-mono)';
export const CODE_FONT_SIZE = '13px';
export const CODE_LINE_HEIGHT = '20px';

const LIGHT = {
  plain: '#2a2520', // 14.4:1
  comment: '#6f6659', //  5.6:1
  keyword: '#a94f2a', //  5.5:1
  string: '#22784f', //  5.3:1
  number: '#8a5a00', //  5.6:1
  func: '#255fb5', //  5.8:1
  type: '#7847b8', //  6.1:1
  operator: '#6e6760', //  5.3:1
  deleted: '#b3261e',
  inserted: '#1f7a3d',
} as const;

const DARK = {
  plain: '#e8e1d2', // 14.3:1
  comment: '#8d8266', //  4.9:1
  keyword: '#e8895f', //  7.3:1
  string: '#7fbf6a', //  8.5:1
  number: '#d9a441', //  8.3:1
  func: '#8fb8e8', //  9.1:1
  type: '#b98ad6', //  6.8:1
  operator: '#b0a892', //  8.3:1
  deleted: '#f07575',
  inserted: '#7ac87c',
} as const;

/**
 * Alma Mater (UCSF) syntax palette — recoloured to UCSF hue families, measured
 * against the Alma code grounds (light #f2f3f4, dark #08213f). `type` is the
 * eggplant accent so code ties to the brand. Every stop clears WCAG AA; ratios
 * asserted in codeTheme.test.ts. See docs/design/alma-mater-theme.md §5g.
 */
const ALMA_LIGHT = {
  plain: '#052049', // Navy
  comment: '#586780', //  4.65:1
  keyword: '#0f388a', //  9.68:1  (A2 Blue)
  string: '#007242', //  5.42:1  (D2 Green)
  number: '#8a5a00', //  5.33:1
  func: '#0e5258', //  7.99:1  (C1 Teal)
  type: '#6c247c', //  8.67:1  (G2 Violet — the accent)
  operator: '#506380', //  5.50:1  (I3)
  deleted: '#c40d3e',
  inserted: '#007242',
} as const;

const ALMA_DARK = {
  plain: '#e1e3e5', // J5
  comment: '#8a93a6', //  5.24:1
  keyword: '#7fb3e6', //  7.30:1
  string: '#6fc084', //  7.35:1
  number: '#e0a44a', //  7.37:1
  func: '#5cc6d0', //  8.04:1  (→C4 teal)
  type: '#c58ad6', //  6.14:1  (→G4 orchid — the accent)
  operator: '#b4b9bf', //  8.18:1  (J3)
  deleted: '#f5768a',
  inserted: '#5fbf74',
} as const;

type SyntaxPalette = {
  plain: string;
  comment: string;
  keyword: string;
  string: string;
  number: string;
  func: string;
  type: string;
  operator: string;
  deleted: string;
  inserted: string;
};

function build(p: SyntaxPalette, tint: string): PrismTheme {
  const base: CSSProperties = {
    color: p.plain,
    background: 'transparent',
    fontFamily: CODE_FONT_FAMILY,
    fontSize: CODE_FONT_SIZE,
    lineHeight: CODE_LINE_HEIGHT,
    // Prism's stock themes ship a text-shadow that smears on a warm ground.
    textShadow: 'none',
    tabSize: 2,
  };

  return {
    'code[class*="language-"]': base,
    'pre[class*="language-"]': { ...base, margin: 0, padding: 0, overflow: 'auto' },

    comment: { color: p.comment, fontStyle: 'italic' },
    prolog: { color: p.comment },
    doctype: { color: p.comment },
    cdata: { color: p.comment },

    punctuation: { color: p.operator },
    operator: { color: p.operator },
    entity: { color: p.operator },
    url: { color: p.func },

    property: { color: p.plain },
    tag: { color: p.keyword },
    'attr-name': { color: p.number },
    'attr-value': { color: p.string },
    selector: { color: p.type },
    atrule: { color: p.keyword },

    boolean: { color: p.number },
    number: { color: p.number },
    constant: { color: p.number },
    symbol: { color: p.number },

    string: { color: p.string },
    char: { color: p.string },
    regex: { color: p.string },

    keyword: { color: p.keyword, fontWeight: 600 },
    'keyword.module': { color: p.keyword, fontWeight: 600 },
    builtin: { color: p.keyword },
    important: { color: p.keyword, fontWeight: 600 },

    function: { color: p.func },
    'class-name': { color: p.type, fontWeight: 600 },
    namespace: { color: p.type },

    variable: { color: p.plain },

    // Diff rows tint the whole line, not just the glyphs.
    deleted: {
      color: p.deleted,
      background: `color-mix(in srgb, ${p.deleted} ${tint}, transparent)`,
    },
    inserted: {
      color: p.inserted,
      background: `color-mix(in srgb, ${p.inserted} ${tint}, transparent)`,
    },
  };
}

export const codeThemeLight = build(LIGHT, '9%');
export const codeThemeDark = build(DARK, '10%');
export const codeThemeAlmaLight = build(ALMA_LIGHT, '9%');
export const codeThemeAlmaDark = build(ALMA_DARK, '10%');

/** Parchment themes, keyed by resolved mode (kept for back-compat). */
export const codeThemes = { light: codeThemeLight, dark: codeThemeDark } as const;

/**
 * Syntax themes keyed by theme family, then resolved mode. Consumers select
 * with `codeThemesByFamily[useThemeFamily()][useResolvedTheme()]` so code
 * matches whichever theme (Parchment / Alma Mater) is active.
 */
export const codeThemesByFamily = {
  parchment: { light: codeThemeLight, dark: codeThemeDark },
  'alma-mater': { light: codeThemeAlmaLight, dark: codeThemeAlmaDark },
} as const;

/** Ground each Alma Mater palette is measured against (--background-muted). */
export const CODE_BG_ALMA = { light: '#f2f3f4', dark: '#08213f' } as const;

/** Palette values, exported so tests can assert the contrast ratios. */
export const codePalettes = { light: LIGHT, dark: DARK } as const;

/** Alma Mater palettes + their grounds, exported for the contrast test. */
export const codePalettesAlma = {
  light: { palette: ALMA_LIGHT, bg: CODE_BG_ALMA.light },
  dark: { palette: ALMA_DARK, bg: CODE_BG_ALMA.dark },
} as const;
