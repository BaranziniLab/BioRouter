#!/usr/bin/env node
/**
 * Visual gate for the composer's WORKING EDGE.
 *
 * WHY THIS EXISTS AS A SEPARATE SCRIPT. The feature is four CSS facts that a
 * unit test cannot see, because jsdom has no layout engine, never runs
 * Tailwind, does not evaluate `:has()`, does not resolve `color-mix()` and does
 * not run `@property`-registered animations. `styles/composerWorkingEdge.test.ts`
 * asserts the DECLARATIONS; this asserts what a browser actually COMPUTES from
 * them, against the real compiled stylesheet, in every theme family and mode.
 *
 * What it proves, per scope (3 families x 2 modes = 6):
 *   1. The sweep exists ONLY while working — idle has no `::after` at all.
 *   2. The sweep animates `br-composer-working` on the ambient-loop tier.
 *   3. focused+idle resolves to the family's FULL accent (D-15, unchanged).
 *   4. focused+working resolves to a DIMMED accent — so the two states are
 *      never the same picture. This is the whole point of the feature: the
 *      composer autofocuses, so without the dim, a full-accent segment would
 *      ride a full-accent edge and read as an ordinary focus border.
 *   5. The accent tracks the FAMILY, so nothing is pinned to coral.
 *
 * Plus, under `prefers-reduced-motion: reduce`:
 *   6. The animation is off — not frozen mid-sweep, which would park a bright
 *      blob on one corner and read as damage.
 *   7. A static DASHED ring is still painted, so "working" is still legible and
 *      still not the solid focus edge.
 *
 * ⚠ `:has()` STYLE INVALIDATION LAGS A FOCUS CHANGE. Reading a computed style
 * in the same task that moved focus returns the PREVIOUS value — measured here
 * repeatedly, and it produces confident, wrong, green-looking numbers. Every
 * read below goes through `readCard`, which waits two animation frames first.
 * Do not "simplify" that away.
 *
 * Usage:  node scripts/verify-composer-working-edge.mjs
 * Exits non-zero on the first failed assertion, and prints a table either way.
 */
import { chromium } from 'playwright';
import { createServer } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HARNESS = path.resolve(HERE, '../.working-edge-harness');

const FAMILIES = ['parchment', 'alma-mater', 'roche-limit'];
const MODES = ['light', 'dark'];

/** The ambient-loop tier (`--dur-slow`, 525ms) taken four times round. */
const EXPECTED_DURATION = '2.1s';
const EXPECTED_ANIMATION = 'br-composer-working';

const failures = [];
const rows = [];

function check(scope, name, actual, predicate, expected) {
  const ok = predicate(actual);
  if (!ok) failures.push(`${scope}: ${name} — expected ${expected}, got ${JSON.stringify(actual)}`);
  return ok;
}

/** An opaque `rgb(...)` with no alpha channel. The full accent looks like this. */
const isOpaque = (color) => /^rgb\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*\)$/.test(color);
/** Anything carrying an alpha below 1 — how the dimmed accent resolves. */
const isTranslucent = (color) => /\/\s*0?\.\d+\s*\)/.test(color) || /rgba\(/.test(color);

async function main() {
  const server = await createServer({
    root: HARNESS,
    publicDir: false,
    plugins: [tailwindcss()],
    server: { port: 0, strictPort: false, fs: { allow: [path.resolve(HARNESS, '..')] } },
    logLevel: 'error',
  });
  await server.listen();
  const { port } = server.httpServer.address();
  const url = `http://localhost:${port}/`;

  const browser = await chromium.launch();

  try {
    for (const reducedMotion of ['no-preference', 'reduce']) {
      const context = await browser.newContext({ reducedMotion });
      const page = await context.newPage();
      await page.goto(url, { waitUntil: 'networkidle' });

      for (const family of FAMILIES) {
        for (const mode of MODES) {
          const scope = `${family}/${mode}${reducedMotion === 'reduce' ? ' (reduced)' : ''}`;
          const m = await page.evaluate(
            async ([fam, md]) => {
              const root = document.documentElement;
              const flush = () =>
                new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));

              if (fam === 'parchment') root.removeAttribute('data-theme');
              else root.setAttribute('data-theme', fam);
              root.classList.toggle('dark', md === 'dark');
              await flush();

              // ⚠ Two frames after ANY focus change. `:has()` invalidation is
              // not synchronous; reading sooner returns the previous value.
              const readCard = async (taId, cardId) => {
                document.querySelectorAll('textarea').forEach((t) => t.blur());
                if (taId) document.getElementById(taId).focus();
                await flush();
                const el = document.getElementById(cardId);
                const after = getComputedStyle(el, '::after');
                return {
                  border: getComputedStyle(el).borderTopColor,
                  content: after.content,
                  animation: after.animationName,
                  duration: after.animationDuration,
                  background: after.backgroundImage,
                };
              };

              return {
                accent: getComputedStyle(root).getPropertyValue('--border-accent').trim(),
                idle: await readCard(null, 'idle'),
                idleFocus: await readCard('ta-idle-focus', 'idle-focus'),
                working: await readCard(null, 'working'),
                workingFocus: await readCard('ta-working-focus', 'working-focus'),
              };
            },
            [family, mode]
          );

          // 1. The sweep exists only while working.
          check(scope, 'idle has no ::after', m.idle.content, (v) => v === 'none', '"none"');
          check(
            scope,
            'working has an ::after',
            m.working.content,
            (v) => v !== 'none',
            'a generated box'
          );

          // 5. The accent is the family's, never a pinned literal.
          check(
            scope,
            'accent token resolves',
            m.accent,
            (v) => /^#[0-9a-f]{6}$/i.test(v),
            'a hex'
          );

          // 3 + 4. THE STATE RELATIONSHIP.
          check(
            scope,
            'focused+idle is the FULL accent',
            m.idleFocus.border,
            isOpaque,
            'an opaque rgb()'
          );
          check(
            scope,
            'focused+working is a DIMMED accent',
            m.workingFocus.border,
            isTranslucent,
            'a translucent colour'
          );
          check(
            scope,
            'focus and working are not the same picture',
            [m.idleFocus.border, m.workingFocus.border],
            ([a, b]) => a !== b,
            'two different borders'
          );

          if (reducedMotion === 'reduce') {
            // 6 + 7.
            check(
              scope,
              'animation is OFF, not frozen',
              m.working.animation,
              (v) => v === 'none',
              '"none"'
            );
            check(
              scope,
              'a static dashed ring is still painted',
              m.working.background,
              (v) => v.includes('repeating-conic-gradient'),
              'repeating-conic-gradient'
            );
          } else {
            // 2.
            check(
              scope,
              'sweep animation name',
              m.working.animation,
              (v) => v === EXPECTED_ANIMATION,
              EXPECTED_ANIMATION
            );
            check(
              scope,
              'sweep runs on the ambient-loop tier',
              m.working.duration,
              (v) => v === EXPECTED_DURATION,
              EXPECTED_DURATION
            );
            check(
              scope,
              'sweep is a conic gradient',
              m.working.background,
              (v) => v.includes('conic-gradient'),
              'conic-gradient'
            );
          }

          rows.push({
            scope,
            accent: m.accent,
            'focus+idle': m.idleFocus.border,
            'focus+working': m.workingFocus.border,
            sweep: `${m.working.animation} ${m.working.duration}`,
          });
        }
      }
      await context.close();
    }
  } finally {
    await browser.close();
    await server.close();
  }

  console.table(rows);

  if (failures.length) {
    console.error(`\n✗ ${failures.length} assertion(s) failed:\n`);
    for (const f of failures) console.error(`  - ${f}`);
    process.exit(1);
  }
  console.log(
    `\n✓ composer working edge verified in ${rows.length} scopes ` +
      `(${FAMILIES.length} families x ${MODES.length} modes x normal/reduced motion)`
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
