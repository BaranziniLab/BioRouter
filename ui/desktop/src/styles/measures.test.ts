import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The reading measures must stay FLUID.
 *
 * They were flat pixel caps, and the symptom was reported as "the app doesn't
 * rescale with the window": at an 1800px window the chat column sat at 760px
 * with roughly 400px of dead band on either side, so dragging the window wider
 * bought margin rather than content.
 *
 * ⚠ **jsdom cannot catch this.** It has no layout engine and never runs
 * Tailwind, so nothing that renders a component can measure a column's width —
 * a regression to `--measure-chat: 760px` would render identically in every
 * other suite in this repo and ship green. The only thing assertable here is
 * the declaration itself, so that is what is asserted, at the source.
 *
 * Measured in a real browser against the built stylesheet when this landed
 * (content pane → column): 560→560, 760→760, 1000→780, 1320→1030, 1560→1180,
 * 2160→1180. The first two are unchanged, which is the point: `max-width`
 * cannot force a box wider than its parent, so the clamp only ever raises the
 * ceiling on a window with room to spare.
 */
const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const READABLE = readFileSync(join(__dirname, '../components/Layout/ReadableContent.tsx'), 'utf8');

function declaration(name: string): string {
  const match = CSS.match(new RegExp(`--${name}:\\s*([^;]+);`));
  if (!match) throw new Error(`--${name} is not declared in main.css`);
  return match[1].trim();
}

describe('reading measures scale with the window', () => {
  it.each(['measure-chat', 'measure-page'])('--%s is a clamp, not a fixed cap', (name) => {
    const value = declaration(name);
    expect(value).toMatch(/^clamp\(/);
    // A clamp of three pixel values would satisfy the line above and still not
    // move: the middle term is what tracks the window.
    expect(value).toMatch(/%/);
  });

  /**
   * ⚠ **A percentage, never `vw`.** These resolve against the containing block,
   * which is the content pane. `vw` is the whole viewport and would over-count
   * by the sidebar's width — widening the column at the exact moment the
   * sidebar opened and took the room away.
   */
  it.each(['measure-chat', 'measure-page'])('--%s tracks the pane, not the viewport', (name) => {
    expect(declaration(name)).not.toMatch(/\dvw/);
  });

  /**
   * The floor must not regress below what shipped, or narrow windows would get
   * NARROWER than they were — the opposite of the complaint.
   */
  it('keeps the old fixed values as the floors', () => {
    expect(declaration('measure-chat')).toMatch(/clamp\(\s*760px/);
    expect(declaration('measure-page')).toMatch(/clamp\(\s*1120px/);
  });

  it('leaves ReadableContent with no fixed pixel cap of its own', () => {
    const caps = READABLE.match(/max-w-\[[^\]]+\]/g) ?? [];
    expect(caps.length).toBeGreaterThan(0);
    for (const cap of caps) expect(cap).toContain('clamp(');
    // …and the chat size stays keyed to the token, so the column and the
    // composer beneath it cannot drift apart.
    expect(READABLE).toContain("chat: 'max-w-measure-chat'");
  });
});
