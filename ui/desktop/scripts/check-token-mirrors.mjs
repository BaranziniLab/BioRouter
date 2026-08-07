#!/usr/bin/env node
/**
 * Mirror guard for the Biorouter design system.
 *
 * A semantic colour token is only half-real when it is declared. `--border-focus`
 * in `:root` gives you `var(--border-focus)`; it does NOT give you
 * `ring-border-focus`. For that, Tailwind needs a `--color-border-focus` entry in
 * the `@theme inline` block — and when that entry is missing the utility is not
 * generated at all, so the class silently does nothing and the element paints
 * whatever it would have painted anyway (`currentcolor` for a ring, the base
 * layer's hairline for a border). Nothing errors. Nothing looks broken in a
 * diff. jsdom cannot see it, because jsdom does not run Tailwind.
 *
 *   node scripts/check-token-mirrors.mjs
 *
 * This shipped three times — `--border-focus`, `--sidebar-ring` and
 * `--background-focus` — across nine call sites and two different stewards, one
 * of whom worked around it with `border-[var(--border-focus)]` (which renders
 * correctly, and so hid the gap instead of reporting it) while another kept
 * writing `ring-border-focus` and got `--text-default`. Both spellings look
 * right in review. Only a browser or this script can tell them apart.
 *
 * SCOPE IS DELIBERATELY NARROW, so this cannot false-positive on work in
 * flight: a token is reported only when it is (1) declared in main.css, (2)
 * actually named by a Tailwind colour utility somewhere in src/, and (3)
 * missing its `--color-*` mirror. An unused token is not a bug, and a token
 * only ever read through `var()` is not one either.
 */
import { readFile, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const CSS = join(ROOT, 'src/styles/main.css');
const SRC = join(ROOT, 'src');

/** Utility prefixes whose value comes from Tailwind's `--color-*` namespace. */
const COLOR_PREFIXES = [
  'bg',
  'text',
  'border',
  'ring',
  'inset-ring',
  'outline',
  'fill',
  'stroke',
  'decoration',
  'divide',
  'shadow',
  'accent',
  'caret',
  'from',
  'via',
  'to',
];

const css = await readFile(CSS, 'utf8');

/**
 * Every `--name:` declared anywhere outside the `@theme` / `@theme inline`
 * blocks — i.e. the semantic layer a family block can re-point.
 */
const declared = new Set();
for (const m of css.matchAll(/^\s{2,}(--[a-z0-9-]+)\s*:/gm)) declared.add(m[1].slice(2));

/** The `@theme inline` mirror block: `--color-x: var(--x)`. */
const inlineBlock = css.slice(css.indexOf('@theme inline'));
const mirrored = new Set();
for (const m of inlineBlock.matchAll(/^\s+--color-([a-z0-9-]+)\s*:/gm)) mirrored.add(m[1]);

/** Walk src/ for class strings. */
async function* files(dir) {
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name === 'node_modules' || e.name.startsWith('.')) continue;
      yield* files(p);
    } else if (/\.(tsx?|css|html)$/.test(e.name)) {
      yield p;
    }
  }
}

// `ring-border-focus`, `hover:bg-overlay-hover`, `dark:text-text-muted/50`, …
const UTILITY = new RegExp(
  String.raw`(?<![\w-])(?:[a-z-]+:)*(${COLOR_PREFIXES.join('|')})-([a-z][a-z0-9-]*?)(?:\/\d+)?(?![\w/-])`,
  'g'
);

const used = new Map(); // token -> Set<file>
for await (const file of files(SRC)) {
  if (file === CSS) continue;
  const text = await readFile(file, 'utf8');
  for (const [, , rest] of text.matchAll(UTILITY)) {
    // Longest declared token that this utility could be naming. `border-focus`
    // must win over `focus` for `border-border-focus`.
    for (let name = rest; name.includes('-'); name = name.slice(name.indexOf('-') + 1)) {
      if (declared.has(name)) {
        if (!used.has(name)) used.set(name, new Set());
        used.get(name).add(relative(ROOT, file));
        break;
      }
    }
    if (declared.has(rest)) {
      if (!used.has(rest)) used.set(rest, new Set());
      used.get(rest).add(relative(ROOT, file));
    }
  }
}

const broken = [...used.keys()].filter((t) => !mirrored.has(t)).sort();

if (broken.length === 0) {
  console.log(`OK — every semantic colour token used by a utility has a @theme inline mirror`);
  console.log(
    `     (${declared.size} declared, ${mirrored.size} mirrored, ${used.size} reached from a utility)`
  );
  process.exit(0);
}

console.error('MISSING @theme inline MIRRORS\n');
console.error('These tokens are declared in main.css and named by a Tailwind colour utility,');
console.error('but have no `--color-<name>` entry — so the utility is never generated and the');
console.error('call sites below silently render a fallback colour.\n');
for (const token of broken) {
  console.error(`  --${token}`);
  console.error(`      add to @theme inline:  --color-${token}: var(--${token});`);
  for (const f of [...used.get(token)].sort()) console.error(`      used by: ${f}`);
  console.error('');
}
process.exit(1);
