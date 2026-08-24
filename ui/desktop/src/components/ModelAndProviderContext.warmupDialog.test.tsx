import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { cn } from '../utils';

/**
 * The local-model warm-up sheet must fit the window, and the part that overflows
 * must be the DETAIL, never the buttons.
 *
 * The bug: its `DialogContent` had no height ceiling and no scrolling body. The
 * detail block grows with the model — blob path, suitability message, fallback
 * state, context and GPU-memory rows — and a dialog is centred with
 * `top-1/2 -translate-y-1/2`, so once the content exceeded the viewport the
 * sheet grew symmetrically past BOTH edges. Its own footer went off the bottom
 * of the screen, taking "Warm up" and "Keep previous" with it. That is not an
 * ugly dialog, it is an unreachable one: the sheet is modal, so with neither
 * button clickable the only way out is Escape — and the user cannot tell that
 * the choice they were asked to make is still there, below the screen.
 *
 * The fix is a ceiling on the content plus the scroll on the BODY:
 * `flex max-h-[calc(100vh-4rem)] flex-col overflow-hidden` on the DialogContent,
 * and `min-h-0 flex-1 … overflow-y-auto` on the detail block. Header and footer
 * are then flex siblings that keep their intrinsic height, so the controls stay
 * pinned in view and it is the detail that scrolls.
 *
 * ⚠ **This is asserted at the SOURCE, and it has to be.** jsdom has no layout
 * engine: every element reports a zero-sized `getBoundingClientRect()`, no box
 * ever exceeds the viewport, `max-height` clamps nothing, and an overflowing
 * child never scrolls because there is no overflow to detect. A component test
 * that rendered this dialog at a short viewport and asked whether the footer was
 * on-screen would answer the same way with the bug present and with it fixed —
 * it cannot even set a viewport height that means anything. The only thing
 * assertable here is the declaration, so that is what is asserted.
 *
 * ⚠ `ModelAndProviderContext.tsx` is NOT edited by this file; it names this test
 * as its guard in a comment beside the classes below.
 */
const SOURCE = readFileSync(join(__dirname, 'ModelAndProviderContext.tsx'), 'utf8');
const DIALOG_PRIMITIVE = readFileSync(join(__dirname, 'ui/dialog.tsx'), 'utf8');

/** The warm-up dialog's own subtree, so nothing outside it can satisfy a match. */
function warmupDialog(): string {
  const start = SOURCE.indexOf('<DialogContent');
  const end = SOURCE.indexOf('</DialogContent>');
  if (start < 0 || end < 0 || end < start) {
    throw new Error('ModelAndProviderContext.tsx renders no <DialogContent>…</DialogContent>');
  }
  return SOURCE.slice(start, end);
}

function contentClasses(): string {
  const match = warmupDialog().match(/<DialogContent[^>]*className="([^"]*)"/);
  if (!match) throw new Error('the warm-up <DialogContent> carries no literal className');
  return match[1];
}

/**
 * The one scrolling region inside the sheet. Required to be unique: a second
 * scroller would mean the header or the footer had been folded into the scrolled
 * area, which is the failure this whole file is about.
 */
function scrollingBodyClasses(): string {
  const scrollers = [...warmupDialog().matchAll(/className="([^"]*)"/g)]
    .map((match) => match[1])
    .filter((classes) => classes.includes('overflow-y-auto'));
  if (scrollers.length !== 1) {
    throw new Error(
      `expected exactly one scrolling body inside the warm-up dialog, found ${scrollers.length}`
    );
  }
  return scrollers[0];
}

describe('the local-model warm-up dialog fits the window', () => {
  it('caps the dialog height against the viewport', () => {
    const classes = contentClasses().split(/\s+/);
    // A `max-h-*` of any flavour, but it has to be viewport-relative: a ceiling
    // in `rem` would still exceed a short window, which is the case that broke.
    const ceiling = classes.find((token) => token.startsWith('max-h-'));
    expect(ceiling).toBeDefined();
    expect(ceiling).toMatch(/vh|dvh|svh/);
  });

  /**
   * The ceiling alone would merely clip the footer instead of pushing it
   * off-screen — equally unclickable. `flex flex-col` is what makes the header,
   * the scrolling body and the footer share the capped height, and
   * `overflow-hidden` keeps the clamp honest by stopping the content painting
   * outside it.
   */
  it('lays the capped sheet out as a column, with the overflow contained', () => {
    const classes = contentClasses().split(/\s+/);
    expect(classes).toContain('flex');
    expect(classes).toContain('flex-col');
    expect(classes).toContain('overflow-hidden');
  });

  /**
   * ⚠ The column layout is an OVERRIDE, and a silent one if it ever stops
   * winning: the shared `DialogContent` primitive is `grid`, and `flex-col` on a
   * grid container does nothing at all — the body would not flex, `flex-1` would
   * not claim the leftover height, and the sheet would look fixed while the
   * footer went back off-screen. What resolves it is `cn()`/tailwind-merge
   * dropping the base `display` when the call site names another one, so that is
   * asserted against the real `cn`, not assumed.
   */
  it('overrides the dialog primitives grid display rather than sitting beside it', () => {
    const contentSlice = DIALOG_PRIMITIVE.slice(
      DIALOG_PRIMITIVE.indexOf('data-slot="dialog-content"')
    );
    expect(contentSlice).toMatch(/(?<![\w-])grid(?![\w-])/);

    const merged = cn('grid', contentClasses()).split(/\s+/);
    expect(merged).toContain('flex');
    expect(merged).not.toContain('grid');
  });

  it('scrolls the detail block, not the sheet', () => {
    expect(scrollingBodyClasses().split(/\s+/)).toContain('overflow-y-auto');
  });

  /**
   * ⚠ `overflow-y-auto` on its own does not scroll here. A flex item's default
   * `min-height` is `auto`, so the body refuses to shrink below its content, the
   * column grows past the ceiling again and the scrollbar never appears —
   * `min-h-0` is the release valve, and `flex-1` is what hands the body the
   * height the header and footer did not take. The three only work as a set.
   */
  it('lets the body actually shrink, so the scrollbar can appear', () => {
    const classes = scrollingBodyClasses().split(/\s+/);
    expect(classes).toContain('min-h-0');
    expect(classes).toContain('flex-1');
  });
});
