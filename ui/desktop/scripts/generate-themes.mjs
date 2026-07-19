#!/usr/bin/env node
/**
 * Theme code generator.
 *
 *   npm run themes            regenerate every artifact
 *   npm run themes -- --check fail if anything is stale (CI)
 *
 * ONE definition per family in `themes/<id>.theme.mjs` is the source of truth.
 * From it this emits, into marker-delimited regions of existing files:
 *
 *   src/styles/main.css          the family token blocks (light + dark)
 *   src/styles/themes.generated.ts  syntax + terminal palettes, family manifest
 *   index.html                   the pre-hydration family list and splash CSS
 *
 * WHY REGIONS, NOT WHOLE FILES: main.css and index.html carry a great deal of
 * hand-written reasoning that is not derivable from a palette. Owning only a
 * delimited region keeps that prose intact and keeps the diff legible.
 *
 * WHY THE BASE FAMILY'S CSS IS NOT GENERATED: Parchment supplies the bare
 * `:root` / `.dark` blocks, which also carry the 17 structural tokens (radii,
 * motion, z-index) that no theme may vary. Those blocks stay hand-written;
 * Parchment's definition contributes only its palettes and splash values.
 *
 * DERIVED, NEVER AUTHORED — these are the values that used to be written in two
 * to four places and silently drifted:
 *   terminal.background   = the family's own `terminalGround` token
 *   terminal.cursorAccent = the same ground
 *   code ground (CODE_BG) = `--background-code`
 *   splash.bg             = `--background-muted`
 *   surface.*             = the five semantic tokens in SURFACE_TOKENS
 */
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readdir } from 'node:fs/promises';
import { buildScopes, resolveRaw, contrast } from './lib/theme-tokens.mjs';
import {
  SEMANTIC_TOKENS,
  RAW_TOKENS,
  SYNTAX_STOPS,
  TERMINAL_STOPS,
  TERMINAL_FLOORS,
  validateTheme,
} from './lib/theme-contract.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const P = (...p) => join(root, ...p);
const CHECK = process.argv.includes('--check');

/* ── load definitions ── */
const files = (await readdir(P('themes'))).filter((f) => f.endsWith('.theme.mjs')).sort();
const themes = [];
for (const f of files) {
  const mod = await import(new URL(`../themes/${f}`, import.meta.url).href);
  themes.push(mod.default);
}
// Base family first, then the rest alphabetically. Order is observable: it is
// the picker order and the pre-hydration allow-list, which boot-splash.test.ts
// asserts matches THEME_FAMILIES exactly.
themes.sort((a, b) => (a.isBase ? -1 : b.isBase ? 1 : a.id.localeCompare(b.id)));
const base = themes.find((t) => t.isBase);
const families = themes.filter((t) => !t.isBase);
if (!base) throw new Error('no base theme (expected exactly one with isBase: true)');

/* ── validate ── */
const problems = themes.flatMap(validateTheme);
if (problems.length) {
  console.error('Theme definitions are invalid:\n' + problems.map((p) => `  ${p}`).join('\n'));
  process.exit(1);
}

/* ── emit CSS token blocks ── */
const decl = (name, value) => `  --${name}: ${value};`;

function cssBlockFor(theme, mode) {
  const sel =
    mode === 'light' ? `:root[data-theme='${theme.id}']` : `.dark[data-theme='${theme.id}']`;
  const def = theme[mode];
  const lines = [];
  if (mode === 'light') {
    lines.push(
      '  /* Raw palette remap (mode-independent): recolours the stray direct',
      "     utility usages so nothing is left with the base family's bias. The",
      '     semantic tokens below are authoritative; these only catch components',
      '     that reach past the semantic layer. */'
    );
    for (const t of RAW_TOKENS) lines.push(decl(t, def.raw[t]));
    lines.push('');
  }
  lines.push(`  /* Semantic tokens — ${theme.label} ${mode.toUpperCase()} */`);
  for (const t of SEMANTIC_TOKENS) lines.push(decl(t, def.tokens[t]));
  return `${sel} {\n${lines.join('\n')}\n}`;
}

// Source order is load-bearing: the two selectors have identical specificity
// (0,2,0), so the dark block MUST follow the light one. Emitting them as a pair
// in this order is what makes that structural rather than a convention.
const cssRegion = families
  .map((t) => `${cssBlockFor(t, 'light')}\n\n${cssBlockFor(t, 'dark')}`)
  .join('\n\n');

function replaceRegion(src, name, body, file) {
  const start = `/* THEMES:GENERATED:${name}:START */`;
  const end = `/* THEMES:GENERATED:${name}:END */`;
  const i = src.indexOf(start);
  const j = src.indexOf(end);
  if (i === -1 || j === -1) {
    throw new Error(`missing ${name} region markers in ${file}`);
  }
  // Without these three checks a malformed region fails OPEN. Swapped markers
  // make the slice re-emit the surrounding text, growing the file on every run
  // and exiting 0; duplicated markers leave a stale second region that WINS the
  // CSS cascade while --check certifies the file current, because both sides
  // use first-occurrence indexOf.
  if (j < i) {
    throw new Error(`${name} region markers are reversed in ${file} (END precedes START)`);
  }
  if (src.indexOf(start, i + 1) !== -1 || src.indexOf(end, j + 1) !== -1) {
    throw new Error(
      `${name} region markers appear more than once in ${file}; a duplicate region ` +
        `would win the cascade while --check reported the file current`
    );
  }
  return src.slice(0, i + start.length) + '\n' + body + '\n' + src.slice(j);
}

/* ── resolve against the CSS we are about to WRITE ──
   Derived values (terminal ground, code ground, splash ground) must come from
   the new token blocks, not from whatever is currently on disk. On the first
   run after the markers are introduced the on-disk region is empty, so
   resolving against it would silently yield undefined. */
const currentCss = await readFile(P('src/styles/main.css'), 'utf8');
const nextCss = replaceRegion(currentCss, 'FAMILIES', cssRegion, 'main.css');
const nextScopes = buildScopes(nextCss);

const scopeFor = (theme, mode) => {
  const key = `${theme.id}:${mode}`;
  const scope = nextScopes[key];
  if (!scope) throw new Error(`no resolved scope for ${key} — is the family block emitted?`);
  return scope;
};
const ground = (theme, mode) => resolveRaw(theme.terminalGround[mode], scopeFor(theme, mode));
const codeGround = (theme, mode) => resolveRaw('--background-code', scopeFor(theme, mode));

/**
 * The semantic tokens that a sandboxed surface has to inline as a literal hex.
 *
 * A `srcdoc` iframe under `default-src 'none'` cannot load the app stylesheet,
 * so `var(--text-default)` resolves to nothing inside it — the document paints
 * unstyled black-on-white. Those surfaces used to carry hand-picked hexes that
 * only knew light vs dark, which pinned every family to Parchment's ground.
 * Resolving the same tokens here keeps one source of truth: change a family's
 * `background-default` and the notebook/spreadsheet previews follow.
 */
const SURFACE_TOKENS = {
  background: '--background-default',
  foreground: '--text-default',
  muted: '--text-muted',
  card: '--background-card',
  border: '--border-subtle',
};
const surface = (theme, mode) =>
  Object.fromEntries(
    Object.entries(SURFACE_TOKENS).map(([key, token]) => [
      key,
      resolveRaw(token, scopeFor(theme, mode)),
    ])
  );

const obj = (o, indent = 4) =>
  Object.entries(o)
    .map(([k, v]) => `${' '.repeat(indent)}${/^[a-z]\w*$/i.test(k) ? k : `'${k}'`}: '${v}',`)
    .join('\n');

function tsFor(theme) {
  const parts = [];
  for (const mode of ['light', 'dark']) {
    const d = theme[mode];
    const g = ground(theme, mode);
    const terminal = { background: g, cursorAccent: g };
    for (const s of TERMINAL_STOPS) terminal[s] = d.terminal[s];
    parts.push(`  ${mode}: {
    mark: { navy: '${d.mark.navy}', coral: '${d.mark.coral}' },
    syntax: {
${obj(d.syntax, 6)}
    },
    codeGround: '${codeGround(theme, mode)}',
    surface: {
${obj(surface(theme, mode), 6)}
    },
    terminalGround: '${theme.terminalGround[mode]}',
    terminal: {
${obj(terminal, 6)}
    },
  },`);
  }
  return `  '${theme.id}': {
    id: '${theme.id}',
    label: '${theme.label}',
    swatch: '${theme.swatch}',
${parts.join('\n')}
  },`;
}

const tsModule = `/**
 * GENERATED — do not edit. Run \`npm run themes\` after changing a theme
 * definition in themes/*.theme.mjs.
 *
 * Carries the theme values that cannot live in CSS: xterm paints to a canvas
 * and react-syntax-highlighter takes a JS object, so neither can read a
 * custom property. Everything here is derived from the same definition that
 * produced the CSS token blocks, which is what keeps the two in step — these
 * values were previously hand-copied and drifted silently.
 */

export type SyntaxPalette = {
  plain: string; comment: string; keyword: string; string: string; number: string;
  func: string; type: string; operator: string; deleted: string; inserted: string;
};

export type TerminalPalette = {
  background: string; foreground: string; cursor: string; cursorAccent: string;
  selectionBackground: string;
  black: string; red: string; green: string; yellow: string;
  blue: string; magenta: string; cyan: string; white: string;
  brightBlack: string; brightRed: string; brightGreen: string; brightYellow: string;
  brightBlue: string; brightMagenta: string; brightCyan: string; brightWhite: string;
};

export type ThemeModeData = {
  /**
   * The brand mark's two inks for this family and mode. The boot splash paints
   * these before React exists, and BioRouterMark paints them after — reading
   * both from here is what stops the two disagreeing. Roche Limit used to flash
   * an orange mark that React then re-rendered in Parchment's coral.
   */
  mark: { navy: string; coral: string };
  syntax: SyntaxPalette;
  /** The surface the syntax palette is measured against (--background-code). */
  codeGround: string;
  /**
   * Resolved literal values for the five semantic tokens a CSP-sandboxed
   * surface has to inline.
   *
   * A \`srcdoc\` iframe under \`default-src 'none'\` cannot reach the app
   * stylesheet, so \`var(--text-default)\` is empty inside it. The notebook and
   * spreadsheet previews build their own document and must write real hexes —
   * these, so the inlined values track the family instead of being frozen to
   * Parchment. Do not add hand-picked colours next to a preview; add the token
   * to SURFACE_TOKENS in scripts/generate-themes.mjs instead.
   */
  surface: {
    /** --background-default — the document ground. */
    background: string;
    /** --text-default — body ink, held to 4.5:1 against \`background\`. */
    foreground: string;
    /** --text-muted — secondary ink. */
    muted: string;
    /** --background-card — a table/cell ground that sits on \`background\`. */
    card: string;
    /** --border-subtle — hairlines and table cell rules. */
    border: string;
  };
  /** Which token the terminal dock paints. Families genuinely differ. */
  terminalGround: string;
  terminal: TerminalPalette;
};

export type GeneratedTheme = {
  id: string; label: string; swatch: string;
  light: ThemeModeData; dark: ThemeModeData;
};

export const GENERATED_THEMES = {
${themes.map(tsFor).join('\n')}
} as const satisfies Record<string, GeneratedTheme>;

export type ThemeFamilyId = keyof typeof GENERATED_THEMES;

/** Every family id, in definition order. The one list. */
export const THEME_FAMILY_IDS = Object.keys(GENERATED_THEMES) as ThemeFamilyId[];
`;

/* ── emit the index.html regions ── */
const splashRules = themes
  .map((t) => {
    const out = [];
    for (const mode of ['light', 'dark']) {
      // The base family's light values live in the `#br-boot` block itself, so
      // re-emitting them here would duplicate a ground — which the splash test
      // reads as one family inheriting another's.
      if (t.isBase && mode === 'light') continue;
      const bg = resolveRaw('--background-muted', scopeFor(t, mode));
      const sel = t.isBase
        ? 'html.dark #br-boot'
        : mode === 'light'
          ? `html[data-theme='${t.id}'] #br-boot`
          : `html.dark[data-theme='${t.id}'] #br-boot`;
      const s = t[mode].mark;
      out.push(
        `      ${sel} {\n` +
          `        --br-bg: ${bg};\n` +
          `        --br-navy: ${s.navy};\n` +
          `        --br-coral: ${s.coral};\n` +
          `        --br-track: ${s.track};\n` +
          `      }`
      );
    }
    return out.join('\n');
  })
  .join('\n');

const familiesJs = `          const FAMILIES = [${themes.map((t) => `'${t.id}'`).join(', ')}];`;

/* ── write regions ── */

const outputs = [];

outputs.push([P('src/styles/main.css'), nextCss]);
outputs.push([P('src/styles/themes.generated.ts'), tsModule]);

let html = await readFile(P('index.html'), 'utf8');
// CSS comments, not HTML comments: `<!--` inside a <style> is a CDO token and
// makes the CSS parser discard the following rule.
html = replaceRegion(html, 'SPLASH', splashRules, 'index.html');
// The family list lives inside a <script>, so its markers are JS comments.
html = replaceRegion(html, 'FAMILYLIST', familiesJs, 'index.html');
outputs.push([P('index.html'), html]);

/* ── contrast validation: a theme that fails cannot be emitted ── */
const failures = [];
{
  // Validate against the CSS we are about to write, not the one on disk.
  const scopes = nextScopes;
  for (const t of themes) {
    for (const mode of ['light', 'dark']) {
      const scope = scopes[`${t.id}:${mode}`];
      if (!scope) continue;
      const cg = resolveRaw('--background-code', scope);
      for (const s of SYNTAX_STOPS) {
        const c = contrast(t[mode].syntax[s], cg);
        if (c < 4.5) {
          failures.push(
            `${t.id}.${mode}: syntax "${s}" ${t[mode].syntax[s]} on code ground ${cg} = ${c.toFixed(2)}:1 (need 4.5)`
          );
        }
      }
      const tg = resolveRaw(t.terminalGround[mode], scope);
      for (const s of TERMINAL_STOPS) {
        const floor = TERMINAL_FLOORS[s]?.floor ?? 4.5;
        if (floor <= 1) continue;
        const c = contrast(t[mode].terminal[s], tg);
        if (c < floor) {
          failures.push(
            `${t.id}.${mode}: terminal "${s}" ${t[mode].terminal[s]} on ground ${tg} = ${c.toFixed(2)}:1 (need ${floor})`
          );
        }
      }

      // The sandboxed previews (notebook HTML output, spreadsheet sheets) paint
      // `surface.foreground` on `surface.background` inside an iframe that
      // cannot load the app stylesheet, so nothing downstream can rescue a bad
      // pair — no cascade, no user stylesheet, just body text on a ground. Hold
      // it to the normal 4.5:1 body-text floor here, where a family is defined.
      const surfaceBg = resolveRaw('--background-default', scope);
      for (const [ink, token] of [
        ['foreground', '--text-default'],
        ['muted', '--text-muted'],
      ]) {
        const value = resolveRaw(token, scope);
        const c = contrast(value, surfaceBg);
        if (c < 4.5) {
          failures.push(
            `${t.id}.${mode}: sandboxed surface "${ink}" ${value} on ${surfaceBg} = ${c.toFixed(2)}:1 (need 4.5)`
          );
        }
      }

      // The boot splash paints the BR mark before React exists. Its two inks
      // must be legible on the splash ground — which is NOT automatic: a
      // regression once left every dark splash carrying the LIGHT navy, making
      // the mark's navy half invisible (1.02:1) on all three families. The mark
      // is a logotype, so it takes the 3:1 graphical floor rather than 4.5.
      // Only the PRIMARY ink is checked. WCAG exempts logotypes from contrast
      // (SC 1.4.11), and the coral half is exactly that — a brand accent that
      // is 2.84:1 (Alma) and 2.81:1 (Roche) on their light grounds by design.
      // The navy half is different: it carries the mark's readable structure,
      // and a regression once left every dark splash using the LIGHT navy,
      // rendering it invisible at 1.02:1 on all three families with nothing
      // failing. That is the case this assertion exists to catch.
      const splashBg = resolveRaw('--background-muted', scope);
      const c = contrast(t[mode].mark.navy, splashBg);
      if (c < 3) {
        failures.push(
          `${t.id}.${mode}: splash navy ${t[mode].mark.navy} on ${splashBg} = ${c.toFixed(2)}:1 (need 3)`
        );
      }
    }
  }
}
if (failures.length) {
  console.error(
    'Refusing to emit — theme contrast failures:\n' + failures.map((f) => `  ${f}`).join('\n')
  );
  process.exit(1);
}

/* ── format, so the generator and the formatter cannot fight ── */
// Emitting unformatted TS would make `npm run themes` and `prettier --check`
// permanently disagree: one would rewrite what the other just fixed.
const { format, resolveConfig } = await import('prettier');
for (let i = 0; i < outputs.length; i++) {
  const [path, content] = outputs[i];
  if (!path.endsWith('.ts')) continue;
  const cfg = (await resolveConfig(path)) ?? {};
  outputs[i] = [path, await format(content, { ...cfg, filepath: path })];
}

/* ── write or check ── */
let stale = 0;
for (const [path, content] of outputs) {
  let existing = null;
  try {
    existing = await readFile(path, 'utf8');
  } catch {}
  if (existing === content) continue;
  if (CHECK) {
    console.error(`STALE: ${path.replace(root + '/', '')}`);
    stale++;
  } else {
    await writeFile(path, content);
    console.log(`wrote ${path.replace(root + '/', '')}`);
  }
}

if (CHECK && stale) {
  console.error(`\n${stale} generated artifact(s) are stale — run \`npm run themes\`.`);
  process.exit(1);
}
console.log(
  CHECK
    ? `OK — generated artifacts are current (${themes.length} themes)`
    : `OK — ${themes.length} themes generated`
);
