// ui/desktop/src/styles/knowledgeLadder.test.ts
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The Knowledge pane's container-query ladder, asserted AT THE SOURCE.
 *
 * ⚠ **jsdom cannot catch what this guards.** It has no layout engine and does
 * not evaluate `@container` at all, so a component test renders every step
 * simultaneously and measures nothing — the two real overflow seams this ladder
 * has had were both found by sweeping pane widths in a browser and were both
 * invisible to the suite. This file therefore does what `styles/measures.test.ts`
 * and `styles/toastLayer.test.ts` do: assert the declaration itself.
 *
 * ⚠ **A `@container` condition cannot read a custom property.** The thresholds
 * exist twice on purpose — once as a documented token, once as the literal the
 * query actually obeys — and drift between the two is silent, because the token
 * is inert. Pinning them together is the whole point of the first test.
 */

const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const STRIP = readFileSync(
  join(__dirname, '../components/knowledge/graph/GraphFacetStrip.tsx'),
  'utf8'
);

/** Every step of R-08's ladder: the token that documents it, and its query. */
const LADDER: { token: string; px: number; query: RegExp }[] = [
  {
    token: '--knowledge-pane-two-col',
    px: 860,
    query: /@container br-knowledge-pane \(min-width: 860px\)/,
  },
  {
    token: '--knowledge-pane-legend-card',
    px: 940,
    query: /@container br-knowledge-pane \(min-width: 940px\)/,
  },
  {
    token: '--knowledge-pane-full-filters',
    px: 1140,
    query: /@container br-knowledge-pane \(min-width: 1140px\)/,
  },
  {
    token: '--knowledge-pane-three-col',
    px: 1400,
    query: /@container br-knowledge-pane \(min-width: 1400px\)/,
  },
  {
    token: '--knowledge-pane-short',
    px: 620,
    query: /@container br-knowledge-pane \(max-height: 620px\)/,
  },
];

describe('the Knowledge pane ladder', () => {
  it.each(LADDER)('$token declares $px and its query obeys the same number', ({ token, px, query }) => {
    expect(CSS).toContain(`${token}: ${px}px;`);
    expect(CSS).toMatch(query);
  });

  it('has no @container step the ladder above does not account for', () => {
    const found = [...CSS.matchAll(/@container br-knowledge-pane \((?:min-width|max-height): (\d+)px\)/g)].map(
      (m) => Number(m[1])
    );
    expect([...new Set(found)].sort((a, b) => a - b)).toEqual(
      [...new Set(LADDER.map((s) => s.px))].sort((a, b) => a - b)
    );
  });

  /**
   * ⚠ **`Predicate` must NOT be core.** At a 946px pane the row measured
   * search (200) + Type (76) + Predicate (105) + More (78) + Legend (93) plus
   * gaps = 576px in a 532px column and clipped `Legend` mid-word. The seam is
   * that 940px widens the sources rail — narrowing this column — while the
   * filter row sheds nothing until the full-filters step. Only `Type` holds the
   * always-visible slot.
   */
  it('keeps exactly one facet in the always-visible slot', () => {
    const core = [...STRIP.matchAll(/(\w+)Facet\('br-facet-core'\)/g)].map((m) => m[1]);
    expect(core).toEqual(['type']);
  });

  it('offers every folded facet inside the More menu', () => {
    const extra = [...STRIP.matchAll(/(\w+)Facet\('br-facet-extra'\)/g)].map((m) => m[1]);
    expect(extra.sort()).toEqual(['predicate', 'source', 'status']);
    // A facet that is `extra` but absent from `More` is unreachable between the
    // two-column step and the full-filters step — a filter that exists and
    // cannot be opened, which is worse than one that is merely hidden.
    const menu = [...STRIP.matchAll(/(\w+)Facet\('', 'knowledge-graph-facet-\w+-in-menu'\)/g)].map(
      (m) => m[1]
    );
    expect(menu.sort()).toEqual(extra.sort());
  });
});
