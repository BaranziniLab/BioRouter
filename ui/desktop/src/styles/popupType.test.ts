import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The composer's popups share one rail, so their type must come from one scale.
 *
 * ⚠ **jsdom cannot check this and never will.** It has no layout engine and
 * never runs the Tailwind compiler, so `text-[10px]` and `text-supporting`
 * produce byte-identical DOM apart from the class attribute, and
 * `getComputedStyle` reports the same thing for both. A component test that
 * rendered a popup and read a font size would pass whichever token was there.
 * The only thing that can hold this is an assertion on the source, which is why
 * this file reads the files rather than rendering them (same reasoning as
 * `measures.test.ts`).
 *
 * The drift being prevented is measured, not hypothetical: the row label in
 * these popups rendered at 13px, 14px and 12px in four popups on the same rail,
 * because two of them restated a size the shared menu row already owned.
 */
const DIR = join(__dirname, '..', 'components', 'bottom_menu');

const popups = readdirSync(DIR).filter((f) => f.endsWith('.tsx') && !f.endsWith('.test.tsx'));

describe('composer popup typography', () => {
  it('has popups to check', () => {
    expect(popups.length).toBeGreaterThan(3);
  });

  it.each(popups)('%s uses the type scale, not raw pixel sizes', (file) => {
    const src = readFileSync(join(DIR, file), 'utf8');
    // Strip comments so a note explaining the rule cannot trip it — this repo
    // has twice shipped a guard that matched its own prose.
    const code = src
      .split('\n')
      .filter((l) => !l.trim().startsWith('//') && !l.trim().startsWith('*'))
      .join('\n');

    const raw = code.match(/text-\[\d+px\]/g);
    expect(raw, `${file} pins a raw font size; use a token from main.css:103-170`).toBeNull();
  });

  it.each(popups)('%s does not re-pin font-sans, which the body already sets', (file) => {
    const code = readFileSync(join(DIR, file), 'utf8')
      .split('\n')
      .filter((l) => !l.trim().startsWith('//') && !l.trim().startsWith('*'))
      .join('\n');
    expect(code.includes('font-sans')).toBe(false);
  });
});
