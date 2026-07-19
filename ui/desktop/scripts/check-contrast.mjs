#!/usr/bin/env node
/**
 * Contrast guard for the Biorouter design system.
 *
 * Parses the real token declarations out of src/styles/main.css, resolves the
 * var() chains, and asserts the WCAG 2.x contrast ratios that design.md promises.
 * Fails the build if any pair regresses.
 *
 *   node scripts/check-contrast.mjs
 *
 * See design.md §3.1 (Colour) and §3.8 (Focus).
 */
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const CSS_PATH = join(here, '..', 'src', 'styles', 'main.css');

/** Return the body of every block whose selector matches `test`. */
function blocks(css, test) {
  const out = [];
  const re = /(^|\n)([^\n{}]+)\{/g;
  let m;
  while ((m = re.exec(css))) {
    const selector = m[2].trim();
    if (!test(selector)) continue;
    // brace-match forward from the opening brace
    let depth = 1;
    let i = re.lastIndex;
    for (; i < css.length && depth > 0; i++) {
      if (css[i] === '{') depth++;
      else if (css[i] === '}') depth--;
    }
    out.push(css.slice(re.lastIndex, i - 1));
  }
  return out;
}

function parseDecls(body) {
  const o = {};
  // strip nested blocks (e.g. @keyframes inside @theme) before reading decls
  const flat = body.replace(/[^{}]*\{[^{}]*\}/g, '');
  for (const m of flat.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) o[m[1]] = m[2].trim();
  return o;
}

const css = await readFile(CSS_PATH, 'utf8');
const THEME = Object.assign({}, ...blocks(css, (s) => s.startsWith('@theme')).map(parseDecls));
const LIGHT = Object.assign({}, ...blocks(css, (s) => s === ':root').map(parseDecls));
const DARK = Object.assign({}, ...blocks(css, (s) => s === '.dark').map(parseDecls));

// Alma Mater (UCSF) theme family. It re-declares only colour tokens in scopes
// with higher specificity, so the effective values are the Parchment cascade
// with the Alma overrides layered on: alma-light = :root < :root[alma];
// alma-dark = :root < .dark < :root[alma] < .dark[alma].
const ALMA_L = Object.assign(
  {},
  ...blocks(css, (s) => s === ":root[data-theme='alma-mater']").map(parseDecls)
);
const ALMA_D = Object.assign(
  {},
  ...blocks(css, (s) => s === ".dark[data-theme='alma-mater']").map(parseDecls)
);
const ALMA_LIGHT = Object.assign({}, LIGHT, ALMA_L);
const ALMA_DARK = Object.assign({}, LIGHT, DARK, ALMA_L, ALMA_D);

// Roche Limit (JupyterLab-inspired) theme family — same cascade rule as Alma.
const ROCHE_L = Object.assign(
  {},
  ...blocks(css, (s) => s === ":root[data-theme='roche-limit']").map(parseDecls)
);
const ROCHE_D = Object.assign(
  {},
  ...blocks(css, (s) => s === ".dark[data-theme='roche-limit']").map(parseDecls)
);
const ROCHE_LIGHT = Object.assign({}, LIGHT, ROCHE_L);
const ROCHE_DARK = Object.assign({}, LIGHT, DARK, ROCHE_L, ROCHE_D);

function resolve(name, scope) {
  let v = scope[name] ?? THEME[name];
  for (let i = 0; i < 12 && v && v.startsWith('var('); i++) {
    const inner = v.slice(4, v.indexOf(')')).trim();
    v = scope[inner] ?? THEME[inner];
  }
  return v && /^#[0-9a-f]{6}$/i.test(v.trim()) ? v.trim().toLowerCase() : null;
}

const lum = (h) => {
  const c = [1, 3, 5]
    .map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};

let failures = 0;
let checks = 0;
const rows = [];

function assert(label, fg, bg, min, scope) {
  const f = resolve(fg, scope);
  const b = resolve(bg, scope);
  if (!f || !b) {
    failures++;
    rows.push(['UNRESOLVED', '', label, `${!f ? fg : bg} does not resolve to a hex`]);
    return;
  }
  const r = ratio(f, b);
  checks++;
  const ok = r >= min;
  if (!ok) failures++;
  rows.push([ok ? 'pass' : 'FAIL', `${r.toFixed(2)}:1`, label, `${f} on ${b} (need ${min})`]);
}

// Grounds that body text can legitimately land on.
const TEXT_GROUNDS = [
  '--background-app',
  '--background-default',
  '--background-muted',
  '--sidebar',
];
// Focus is a surface shift (D-15); the ring is only drawn under `prefers-contrast:
// more` / `forced-colors`. When it IS drawn it sits outside the control, so it is
// measured against the page ground rather than the control's own fill.
const RING_GROUNDS = [...TEXT_GROUNDS, '--background-medium'];

for (const [theme, scope] of [
  ['light', LIGHT],
  ['dark', DARK],
  ['alma-light', ALMA_LIGHT],
  ['alma-dark', ALMA_DARK],
  ['roche-light', ROCHE_LIGHT],
  ['roche-dark', ROCHE_DARK],
]) {
  rows.push(['', '', `── ${theme} ──`, '']);
  for (const g of TEXT_GROUNDS) {
    assert(`${theme}: text-default on ${g}`, '--text-default', g, 4.5, scope);
    assert(`${theme}: text-muted on ${g}`, '--text-muted', g, 4.5, scope);
    assert(`${theme}: text-subtle on ${g}`, '--text-subtle', g, 4.5, scope);
  }
  for (const g of RING_GROUNDS) assert(`${theme}: focus ring on ${g}`, '--ring', g, 3.0, scope);

  assert(
    `${theme}: text-on-accent ON accent`,
    '--text-on-accent',
    '--background-accent',
    4.5,
    scope
  );
  assert(`${theme}: text-accent as text`, '--text-accent', '--background-app', 4.5, scope);

  for (const s of ['danger', 'success', 'warning', 'info']) {
    assert(`${theme}: text-${s} on app`, `--text-${s}`, '--background-app', 4.5, scope);
    assert(
      `${theme}: text-on-status ON ${s} fill`,
      '--text-on-status',
      `--background-${s}`,
      4.5,
      scope
    );
  }

  // A hairline must be perceivable against its own ground, though it is not "text".
  assert(`${theme}: border-subtle vs app`, '--border-subtle', '--background-app', 1.25, scope);
  assert(`${theme}: border-strong vs subtle`, '--border-strong', '--border-subtle', 1.1, scope);

  // The code ground (design.md §5.1, P6 "the monospace layer is part of the
  // design system"). This guard previously never looked at it, which is how
  // dark code blocks came to paint --background-muted (#282217) while their
  // syntax palette was verified against #16120c: `comment` rendered at 4.15:1,
  // under AA, and nothing failed. --background-code now names the ground the
  // palette was measured on; these assertions keep the two from drifting apart
  // again. The per-token syntax stops are asserted in codeTheme.test.ts.
  // (No border-vs-code-ground assertion: a code block's hairline is perceived
  // against the PAGE ground it sits on, which is already asserted above — not
  // against its own fill. In dark, --border-subtle and --background-muted are
  // both neutral-800, so the block is delimited by its ground change alone,
  // exactly as P1 intends.)
  assert(`${theme}: text-default on code ground`, '--text-default', '--background-code', 4.5, scope);
  assert(`${theme}: text-muted on code ground`, '--text-muted', '--background-code', 4.5, scope);
  assert(`${theme}: text-subtle on code ground`, '--text-subtle', '--background-code', 4.5, scope);

  // Focus (D-15) is a surface shift. Text must stay legible on the focused fill,
  // and the focused edge must be distinguishable from the resting one.
  assert(
    `${theme}: text-default on background-focus`,
    '--text-default',
    '--background-focus',
    4.5,
    scope
  );
  assert(
    `${theme}: border-focus vs background-focus`,
    '--border-focus',
    '--background-focus',
    3.0,
    scope
  );
  assert(
    `${theme}: background-focus vs medium (hover)`,
    '--background-focus',
    '--background-medium',
    1.1,
    scope
  );

  // Nav icons are graphical objects, not text: WCAG SC 1.4.11 asks 3:1, and it
  // asks it against every row the icon can sit on — the resting sidebar, the
  // hover fill, and the active fill. The darkest row is what binds. Alma Mater
  // paints these in the brand teal (the one place it appears at reading size),
  // so an accent change that only checked the resting sidebar could ship an
  // icon that disappears the moment a row is selected. Parchment passes these
  // trivially because its --sidebar-icon is a pass-through to the label ink.
  for (const g of ['--sidebar', '--sidebar-hover', '--sidebar-active']) {
    assert(`${theme}: sidebar icon on ${g}`, '--sidebar-icon', g, 3.0, scope);
  }
}

const w = Math.max(...rows.map((r) => r[2].length));
for (const [status, r, label, note] of rows) {
  if (!status) {
    console.log(`\n${label}`);
    continue;
  }
  console.log(`  ${status.padEnd(10)} ${r.padStart(8)}  ${label.padEnd(w)}  ${note}`);
}

console.log(
  `\n${failures ? `FAIL — ${failures} contrast assertion(s) regressed` : `OK — all ${checks} contrast assertions pass`}`
);
process.exit(failures ? 1 : 0);
