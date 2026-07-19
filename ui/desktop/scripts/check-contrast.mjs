#!/usr/bin/env node
/**
 * Contrast guard for the Biorouter design system.
 *
 * Parses the real token declarations out of src/styles/main.css, resolves the
 * var() chains, and asserts the WCAG 2.x contrast ratios that design.md
 * promises. Fails the build if any pair regresses.
 *
 *   node scripts/check-contrast.mjs
 *
 * THEME FAMILIES ARE DISCOVERED, NOT LISTED. The scope table comes from
 * scripts/lib/theme-tokens.mjs, which sweeps the stylesheet for
 * `[data-theme='...']` blocks. Adding a family therefore needs no edit here —
 * it is audited automatically the moment its tokens exist. The previous
 * version hardcoded six scopes built by hand, and matched their selectors by
 * exact string equality: a block written with different quoting or spacing
 * silently yielded `{}`, the scope fell back to pure Parchment, and ~40
 * assertions passed while measuring the wrong theme.
 *
 * It also asserts the CROSS-FILE duplications. Several values are necessarily
 * written twice — xterm paints to canvas and react-syntax-highlighter takes a
 * JS object, so neither can read a CSS var — and nothing used to check that the
 * copies agreed. That is how a syntax palette came to be verified against a
 * surface the app never painted, rendering `comment` at 4.15:1 with everything
 * green.
 *
 * See design.md §3.1 (Colour) and §3.8 (Focus).
 */
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  buildScopes,
  discoverFamilies,
  assertBlockOrder,
  resolveHex,
  contrast as ratioOf,
} from './lib/theme-tokens.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const CSS_PATH = join(here, '..', 'src', 'styles', 'main.css');

const css = await readFile(CSS_PATH, 'utf8');
const SCOPES = buildScopes(css);
const FAMILIES = discoverFamilies(css);

const resolve = (name, scope) => resolveHex(name, scope);
const ratio = ratioOf;

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

// Source order is load-bearing: `:root[data-theme=X]` and `.dark[data-theme=X]`
// have identical specificity (0,2,0), so only document order separates them.
// Swap them and dark mode renders light tokens with every ratio still passing.
for (const problem of assertBlockOrder(css)) {
  rows.push(['FAIL', '', 'block order', problem]);
  failures++;
}

for (const [theme, scope] of Object.entries(SCOPES)) {
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

  // NOT ASSERTED: --accent-bar. It is tempting to hold the active-nav rail to
  // SC 1.4.11's 3:1, and Roche Limit's design doc explicitly guarantees it
  // ("clears 3:1 on every light ground, min 3.14"). That guarantee is false —
  // the rail sits on --sidebar-active, where it measures 2.80 (Roche light),
  // 2.23 (Alma light) and 2.18 (Parchment light). All THREE families fail,
  // which means the design has never held this rule, not that one theme
  // regressed. The rail is decorative reinforcement of a background change the
  // active row already makes, so it is not the sole cue and 1.4.11 does not
  // bite. Asserting it here would fail every theme on day one; the doc claim is
  // what needs correcting. Revisit if the rail ever becomes the only affordance.
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
