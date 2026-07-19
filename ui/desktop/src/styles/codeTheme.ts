/**
 * The Biorouter syntax palette (design.md §5.1, decision D-10).
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
 * accent so code ties to the brand — now the teal C column, which is why `type`
 * and `func` swapped families: the accent teal took `type`, and the eggplant it
 * displaced from the chrome moved to `func`, where its hue distance from teal
 * keeps the two roles legibly apart. (The accent's own C2 #14828c is only
 * 4.11:1 on the code ground and could not be used here; C1 #0e5258 is 7.99:1.)
 * Every stop clears WCAG AA; ratios asserted in codeTheme.test.ts.
 * See docs/design/alma-mater-theme.md §5g.
 */
const ALMA_LIGHT = {
  plain: '#052049', // Navy
  comment: '#586780', //  4.65:1
  keyword: '#0f388a', //  9.68:1  (A2 Blue)
  string: '#007242', //  5.42:1  (D2 Green)
  number: '#8a5a00', //  5.33:1
  func: '#6c247c', //  8.67:1  (G2 Violet)
  type: '#0e5258', //  7.99:1  (C1 Teal — the accent family)
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
  func: '#c58ad6', //  6.14:1  (→G4 orchid)
  type: '#5cc6d0', //  8.04:1  (→C4 teal — the accent family)
  operator: '#b4b9bf', //  8.18:1  (J3)
  deleted: '#f5768a',
  inserted: '#5fbf74',
} as const;

/**
 * Roche Limit syntax palette — JupyterLab's own IPython/Pygments hues, darkened
 * (light) and lifted (dark) until every stop clears WCAG AA on the Roche code
 * grounds (light #f5f5f3, dark #1b1b19). Ratios asserted in codeTheme.test.ts.
 *
 * Two stops deliberately do NOT copy Jupyter: their `comment` (#408080) ships
 * unchanged in dark at ~2.8:1, and their dark `func` (#1e88e5) at ~3.4:1 —
 * both fail AA. See docs/design/roche-limit-theme.md §4.10 / §5.8.
 */
const ROCHE_LIGHT = {
  plain: '#1f1e1c', // 15.26:1
  comment: '#3f6e6e', //  5.25:1  (Jupyter #408080 teal, darkened)
  keyword: '#0a7a32', //  5.01:1  (Jupyter #008000 green)
  string: '#b02121', //  6.23:1  (Jupyter #ba2121 brick)
  number: '#0f6e38', //  5.82:1
  func: '#1849b8', //  7.17:1  (Jupyter #0000ff def-blue)
  type: '#0f6e38', //  5.82:1  (Jupyter #008000 builtin)
  operator: '#7024b0', //  7.49:1  (Jupyter #7800c2)
  deleted: '#c4232b',
  inserted: '#12805c',
} as const;

const ROCHE_DARK = {
  plain: '#ededea', // 14.71:1
  comment: '#7fa3a3', //  6.30:1  (Jupyter #408080 lifted)
  keyword: '#6fcb78', //  8.62:1  (Jupyter #4caf50)
  string: '#ff8f8f', //  7.87:1  (Jupyter #ff7070)
  number: '#84d089', //  9.34:1  (Jupyter #66bb6a)
  func: '#7fbef7', //  8.72:1  (Jupyter #1e88e5 lifted)
  type: '#84d089', //  9.34:1  (Jupyter #43a047 builtin)
  operator: '#d9a0ff', //  8.56:1  (Jupyter #d48fff)
  deleted: '#ff9592',
  inserted: '#3dd68c',
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
export const codeThemeRocheLight = build(ROCHE_LIGHT, '9%');
export const codeThemeRocheDark = build(ROCHE_DARK, '10%');

/** Parchment themes, keyed by resolved mode (kept for back-compat). */
export const codeThemes = { light: codeThemeLight, dark: codeThemeDark } as const;

/**
 * Syntax themes keyed by theme family, then resolved mode. Consumers select
 * with `codeThemesByFamily[useThemeFamily()][useResolvedTheme()]` so code
 * matches whichever theme (Parchment / Alma Mater / Roche Limit) is active.
 */
export const codeThemesByFamily = {
  parchment: { light: codeThemeLight, dark: codeThemeDark },
  'alma-mater': { light: codeThemeAlmaLight, dark: codeThemeAlmaDark },
  'roche-limit': { light: codeThemeRocheLight, dark: codeThemeRocheDark },
} as const;

/** Ground each Alma Mater palette is measured against (--background-muted). */
export const CODE_BG_ALMA = { light: '#f2f3f4', dark: '#08213f' } as const;

/**
 * Ground each Roche Limit palette is measured against. Light uses
 * `--background-code` (#f5f5f3), which is deliberately a hair lighter than
 * `--background-muted`; dark uses `--background-code` (#1b1b19), which equals
 * `--background-default`, exactly as both shipping families do.
 */
export const CODE_BG_ROCHE = { light: '#f5f5f3', dark: '#1b1b19' } as const;

/** Palette values, exported so tests can assert the contrast ratios. */
export const codePalettes = { light: LIGHT, dark: DARK } as const;

/** Alma Mater palettes + their grounds, exported for the contrast test. */
export const codePalettesAlma = {
  light: { palette: ALMA_LIGHT, bg: CODE_BG_ALMA.light },
  dark: { palette: ALMA_DARK, bg: CODE_BG_ALMA.dark },
} as const;

/** Roche Limit palettes + their grounds, exported for the contrast test. */
export const codePalettesRoche = {
  light: { palette: ROCHE_LIGHT, bg: CODE_BG_ROCHE.light },
  dark: { palette: ROCHE_DARK, bg: CODE_BG_ROCHE.dark },
} as const;
