#!/usr/bin/env node
/**
 * ONE-SHOT MIGRATION TOOL — reads the current hand-maintained theme values out
 * of main.css / codeTheme.ts / InAppTerminalDock.tsx / index.html and writes
 * one `themes/<id>.theme.mjs` per family.
 *
 * This exists so the move to codegen can be proven safe rather than asserted:
 * extract the current state, generate from it, and diff the generated output
 * against what shipped. Only once that diff is empty are the deliberate fixes
 * applied — to the definitions, where the diff then shows exactly the intended
 * change and nothing else.
 *
 * It is not part of the build. Once the definitions are checked in this file
 * can be deleted; it is kept only so the migration is reproducible.
 *
 *   node scripts/extract-themes.mjs
 */
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildScopes, discoverFamilies, resolveRaw } from './lib/theme-tokens.mjs';
import {
  SEMANTIC_TOKENS,
  RAW_TOKENS,
  SYNTAX_STOPS,
  TERMINAL_STOPS,
  SPLASH_STOPS,
} from './lib/theme-contract.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const P = (...p) => join(root, ...p);

const css = await readFile(P('src/styles/main.css'), 'utf8');
const codeTs = await readFile(P('src/styles/codeTheme.ts'), 'utf8');
const termTsx = await readFile(P('src/components/InAppTerminalDock.tsx'), 'utf8');
const html = await readFile(P('index.html'), 'utf8');

const scopes = buildScopes(css);
const families = ['parchment', ...discoverFamilies(css)];

/** Pull a `const NAME = { ... } as const;` object literal out of a TS source. */
function tsObject(src, name) {
  const re = new RegExp(`const ${name}\\s*=\\s*\\{`);
  const m = src.match(re);
  if (!m) return null;
  let i = m.index + m[0].length;
  let depth = 1;
  for (; i < src.length && depth > 0; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}') depth--;
  }
  return src.slice(m.index + m[0].length, i - 1);
}

/** Read `key: '#value'` pairs, optionally from a nested `light:`/`dark:` block. */
function pairs(body, mode) {
  if (!body) return {};
  let scoped = body;
  if (mode) {
    const m = body.match(new RegExp(`\\b${mode}\\s*:\\s*\\{`));
    if (!m) return {};
    let i = m.index + m[0].length;
    let depth = 1;
    for (; i < body.length && depth > 0; i++) {
      if (body[i] === '{') depth++;
      else if (body[i] === '}') depth--;
    }
    scoped = body.slice(m.index + m[0].length, i - 1);
  }
  const out = {};
  for (const m of scoped.matchAll(/(\w+)\s*:\s*'([^']+)'/g)) out[m[1]] = m[2];
  return out;
}

// Which TS const holds each family's palettes.
const CONST_NAMES = {
  parchment: { syntax: ['LIGHT', 'DARK'], terminal: 'TERMINAL_THEMES' },
  'alma-mater': { syntax: ['ALMA_LIGHT', 'ALMA_DARK'], terminal: 'ALMA_TERMINAL_THEMES' },
  'roche-limit': { syntax: ['ROCHE_LIGHT', 'ROCHE_DARK'], terminal: 'ROCHE_TERMINAL_THEMES' },
};

/** The base `#br-boot` declarations every family inherits from. */
const splashBase = (() => {
  const m = html.match(/#br-boot\s*\{([\s\S]*?)\n\s*\}/);
  const out = {};
  if (m) for (const mm of m[1].matchAll(/--br-([\w-]+)\s*:\s*([^;]+);/g)) out[mm[1]] = mm[2].trim();
  return out;
})();

/** Splash values live in `html[data-theme='x'] #br-boot { --br-*: … }` rules. */
function splashFor(family, mode) {
  const sel =
    family === 'parchment'
      ? mode === 'light'
        ? /html:not\(\.dark\)\s*#br-boot\s*\{([^}]*)\}|html\s*#br-boot\s*\{([^}]*)\}/
        : /html\.dark\s*#br-boot\s*\{([^}]*)\}/
      : mode === 'light'
        ? new RegExp(`html\\[data-theme='${family}'\\]\\s*#br-boot\\s*\\{([^}]*)\\}`)
        : new RegExp(`html\\.dark\\[data-theme='${family}'\\]\\s*#br-boot\\s*\\{([^}]*)\\}`);
  const m = html.match(sel);
  const body = m ? (m.slice(1).find(Boolean) ?? '') : '';
  const out = {};
  for (const mm of body.matchAll(/--br-([\w-]+)\s*:\s*([^;]+);/g)) out[mm[1]] = mm[2].trim();
  return out;
}

/** Which token does this family's terminal background actually equal? */
function terminalGround(family, mode, termBg) {
  const scope = scopes[`${family}:${mode}`];
  for (const tok of ['--background-muted', '--background-code', '--background-default']) {
    if (resolveRaw(tok, scope)?.toLowerCase() === termBg?.toLowerCase()) return tok;
  }
  return null;
}

// Shadow values are multi-line in the stylesheet; collapse them so the emitted
// literal stays on one line.
const q = (s) => `'${String(s).replace(/\s+/g, ' ').trim().replace(/'/g, "\\'")}'`;
const block = (obj, indent) =>
  Object.entries(obj)
    .map(([k, v]) => `${' '.repeat(indent)}${/^[a-z][\w]*$/i.test(k) ? k : q(k)}: ${q(v)},`)
    .join('\n');

await mkdir(P('themes'), { recursive: true });

// FAMILIES is an annotated array literal (`const FAMILIES: {...}[] = [ … ]`),
// so read the entries straight out of the source rather than via tsObject.
const swatches = {};
const selectorSrc = await readFile(
  P('src/components/BioRouterSidebar/ThemeFamilySelector.tsx'),
  'utf8'
);
for (const m of selectorSrc.matchAll(
  /\{\s*id:\s*'([^']+)',\s*label:\s*'([^']+)',\s*swatch:\s*'([^']+)'\s*\}/g
)) {
  swatches[m[1]] = { label: m[2], swatch: m[3] };
}

for (const family of families) {
  const names = CONST_NAMES[family];
  const modes = {};
  const grounds = {};

  for (const [i, mode] of ['light', 'dark'].entries()) {
    const scope = scopes[`${family}:${mode}`];
    const tokens = {};
    for (const t of SEMANTIC_TOKENS) {
      if (t === 'scrim') continue; // introduced by the fix pass, not present yet
      const v = resolveRaw(`--${t}`, scope);
      if (v != null) tokens[t] = v;
    }
    const raw = {};
    if (family !== 'parchment') {
      for (const t of RAW_TOKENS) {
        const v = resolveRaw(`--${t}`, scope);
        if (v != null) raw[t] = v;
      }
    }
    const syntaxAll = pairs(tsObject(codeTs, names.syntax[i]));
    const syntax = {};
    for (const s of SYNTAX_STOPS) if (syntaxAll[s]) syntax[s] = syntaxAll[s];

    const termAll = pairs(tsObject(termTsx, names.terminal), mode);
    const terminal = {};
    for (const s of TERMINAL_STOPS) if (termAll[s]) terminal[s] = termAll[s];
    grounds[mode] = terminalGround(family, mode, termAll.background) ?? '--background-muted';

    // Families inherit any splash value they do not restate from the base
    // `#br-boot` rule. Resolve that inheritance now so each generated family
    // declares its own complete set — inherited-by-omission is exactly the kind
    // of implicit coupling that let Roche Limit ship a Parchment scrim.
    const splashAll = { ...splashBase, ...splashFor(family, mode) };
    const splash = {};
    for (const s of SPLASH_STOPS) if (splashAll[s]) splash[s] = splashAll[s];

    modes[mode] = { tokens, raw, syntax, terminal, splash, _termBg: termAll.background };
  }

  const meta = swatches[family] ?? { label: family, swatch: '#888888' };
  const isBase = family === 'parchment';

  const src = `/**
 * ${meta.label} — theme definition.
 *
 * THIS FILE IS THE SOURCE OF TRUTH for the ${meta.label} theme. Everything else
 * (the CSS token blocks, the syntax palette, the terminal palette, the picker
 * entry, the boot splash) is generated from it by scripts/generate-themes.mjs.
 * Do not edit the generated regions — edit here and re-run \`npm run themes\`.
 */

/** @type {import('../scripts/lib/theme-contract.mjs').ThemeDefinition} */
export default {
  id: ${q(family)},
  label: ${q(meta.label)},
  swatch: ${q(meta.swatch)},${isBase ? '\n  isBase: true,' : ''}

  // Which token the terminal dock actually paints. The families genuinely
  // differ here; this records the truth rather than assuming they agree.
  terminalGround: {
    light: ${q(grounds.light)},
    dark: ${q(grounds.dark)},
  },

${['light', 'dark']
  .map(
    (mode) => `  ${mode}: {
    tokens: {
${block(modes[mode].tokens, 6)}
    },
${
  isBase
    ? ''
    : `    raw: {
${block(modes[mode].raw, 6)}
    },
`
}    syntax: {
${block(modes[mode].syntax, 6)}
    },
    terminal: {
${block(modes[mode].terminal, 6)}
    },
    splash: {
${block(modes[mode].splash, 6)}
    },
  },`
  )
  .join('\n')}
};
`;
  await writeFile(P('themes', `${family}.theme.mjs`), src);
  console.log(
    `wrote themes/${family}.theme.mjs  ` +
      `(terminal ground: light=${grounds.light} dark=${grounds.dark})`
  );
}
