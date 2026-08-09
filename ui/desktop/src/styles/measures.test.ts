import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The reading measures. **The two are governed by opposite rules, and that is
 * the whole point of this file.**
 *
 * `--measure-page` must stay FLUID. It governs settings, sessions and the other
 * document-shaped views, where a wider window genuinely buys content — more
 * table columns, more cards per row. It was once a flat cap, and the symptom was
 * reported as "the app doesn't rescale with the window": dragging the window
 * wider bought margin rather than content.
 *
 * `--measure-chat` must stay FLAT at 760px. It was briefly widened into a clamp
 * on the same reasoning, and that was wrong for this measure specifically: a
 * 1180px composer is not a more capable composer, it is a line of prose the eye
 * has to track back across, and it drags the toolbar's controls to opposite ends
 * of the window. The design of record names "the 760px column" as Biorouter's
 * identity as an instrument (docs/design/astryx-adoption/astryx-ui-adoption-design.md
 * §1), so 760 is the measure, not the floor of one.
 *
 * ⚠ **jsdom cannot catch either direction.** It has no layout engine and never
 * runs Tailwind, so nothing that renders a component can measure a column's
 * width — a change to either declaration renders identically in every other
 * suite in this repo and ships green. The only thing assertable here is the
 * declaration itself, so that is what is asserted, at the source.
 *
 * Measured in a real browser against the built stylesheet: with the flat chat
 * measure, the composer and the transcript column both sit at 760px at every
 * window width above 760, and below it they are simply pane-wide — `max-width`
 * cannot force a box wider than its parent.
 */
const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const READABLE = readFileSync(join(__dirname, '../components/Layout/ReadableContent.tsx'), 'utf8');
const MAIN = readFileSync(join(__dirname, '../main.ts'), 'utf8');
const SIDEBAR = readFileSync(join(__dirname, '../components/ui/sidebar.tsx'), 'utf8');

function declaration(name: string): string {
  const match = CSS.match(new RegExp(`--${name}:\\s*([^;]+);`));
  if (!match) throw new Error(`--${name} is not declared in main.css`);
  return match[1].trim();
}

describe('the chat measure is a flat 760px', () => {
  /**
   * Asserted as a whole-string equality rather than a pattern, because the
   * failure this guards against is a *widening* — and every loose matcher
   * (`/760px/`, `/^760/`) is satisfied by `clamp(760px, 78%, 1180px)`, which is
   * precisely the value being ruled out.
   */
  it('is exactly 760px, with no clamp and no percentage term', () => {
    expect(declaration('measure-chat')).toBe('760px');
  });

  /**
   * The composer and the transcript column read the SAME token, and must, or
   * the input would sit at a different width from the messages above it. This
   * asserts the shared key rather than the width, so the two can never drift
   * even if the number changes again.
   */
  it('is the one token the chat column and the composer both key off', () => {
    expect(READABLE).toContain("chat: 'max-w-measure-chat'");
  });
});

describe('the page measure scales with the window', () => {
  it('is a clamp, not a fixed cap', () => {
    const value = declaration('measure-page');
    expect(value).toMatch(/^clamp\(/);
    // A clamp of three pixel values would satisfy the line above and still not
    // move: the middle term is what tracks the window.
    expect(value).toMatch(/%/);
  });

  /**
   * ⚠ **A percentage, never `vw`.** It resolves against the containing block,
   * which is the content pane. `vw` is the whole viewport and would over-count
   * by the sidebar's width — widening the column at the exact moment the
   * sidebar opened and took the room away.
   */
  it('tracks the pane, not the viewport', () => {
    expect(declaration('measure-page')).not.toMatch(/\dvw/);
  });

  /**
   * The floor must not regress below what shipped, or narrow windows would get
   * NARROWER than they were — the opposite of the complaint.
   */
  it('keeps the old fixed value as its floor', () => {
    expect(declaration('measure-page')).toMatch(/clamp\(\s*1120px/);
  });

  /**
   * ReadableContent's OTHER three sizes (text / wide / graph) are page measures
   * and stay fluid. `chat` is excluded — it names the flat token above, so it
   * legitimately carries no `clamp(`.
   */
  it('leaves ReadableContent with no fixed pixel cap of its own', () => {
    const caps = READABLE.match(/max-w-\[[^\]]+\]/g) ?? [];
    expect(caps.length).toBeGreaterThan(0);
    for (const cap of caps) expect(cap).toContain('clamp(');
  });
});

/**
 * The window's minimum width is the one place a measure escapes the stylesheet.
 * It exists because the Home view's usage heatmap is the only element whose
 * size is COMPUTED rather than declared: it fits its cells to the box it is
 * given, so a window narrow enough to squeeze the reading column squeezes the
 * grid with it. The floor is therefore not a taste call — it is sidebar +
 * column, the width at which the reading column first reaches its own measure.
 *
 * ⚠ Not the regression in docs/desktop-ui/window-scaling-regressions.md. That
 * one is a flat `max-width` that stops a WIDE window buying content. This is a
 * floor under a NARROW one and does nothing above it.
 *
 * Asserted as arithmetic, not as three separate literals, so that changing the
 * sidebar width or the chat measure fails here instead of silently leaving the
 * window able to compress the heatmap again.
 */
describe('the minimum window width is derived from the sidebar and the chat measure', () => {
  const px = (value: string): number =>
    value.endsWith('rem') ? parseFloat(value) * 16 : parseFloat(value);

  it('is exactly the sidebar plus the reading column', () => {
    const sidebar = SIDEBAR.match(/const SIDEBAR_WIDTH = '([^']+)'/);
    if (!sidebar) throw new Error('SIDEBAR_WIDTH is not declared in components/ui/sidebar.tsx');

    const minWidth = MAIN.match(/^\s*minWidth: (\d+),$/m);
    if (!minWidth) throw new Error('the main window declares no minWidth');

    expect(Number(minWidth[1])).toBe(px(sidebar[1]) + px(declaration('measure-chat')));
  });

  /**
   * `useContentSize` is what makes the arithmetic above comparable at all: it
   * makes `minWidth` a CONTENT width, the same coordinate space the renderer's
   * sidebar and column live in. Without it the number would be off by the
   * platform's window frame.
   */
  it('is expressed in content coordinates', () => {
    expect(MAIN).toMatch(/useContentSize: true/);
  });
});
