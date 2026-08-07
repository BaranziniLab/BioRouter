import { describe, it, expect, afterEach } from 'vitest';
import {
  clampGhostToViewport,
  countWindowTabs,
  measureStripBands,
  resolveMergeInsertion,
} from './tabTearOffBridge';
import { ChatGroupsState } from './chatGroupsTypes';

/**
 * jsdom measures every box as zero and does not implement `elementFromPoint` AT
 * ALL — not "returns null", missing. Both are stubbed per-test rather than
 * globally, so each case states the geometry it is asserting against instead of
 * inheriting one.
 */
function withLayout(rects: Record<string, DOMRect>, hit: (x: number, y: number) => Element | null) {
  const originalRect = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function measured(this: Element) {
    const key =
      (this as HTMLElement).dataset?.tabId ??
      (this as HTMLElement).dataset?.tabStripGroup ??
      '__none__';
    return rects[key] ?? ({ x: 0, y: 0, width: 0, height: 0, left: 0, top: 0 } as DOMRect);
  };
  (
    document as unknown as { elementFromPoint: (x: number, y: number) => Element | null }
  ).elementFromPoint = hit;
  return () => {
    Element.prototype.getBoundingClientRect = originalRect;
    delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
  };
}

function rect(left: number, top: number, width: number, height: number): DOMRect {
  return { x: left, y: top, left, top, width, height } as DOMRect;
}

let restore: (() => void) | null = null;
afterEach(() => {
  restore?.();
  restore = null;
  document.body.innerHTML = '';
});

describe('measureStripBands', () => {
  it('reports one viewport rectangle per strip, so a split window offers several targets', () => {
    document.body.innerHTML = `
      <div data-tab-strip-group="grp-1"></div>
      <div data-tab-strip-group="grp-2"></div>
    `;
    restore = withLayout(
      { 'grp-1': rect(0, 0, 600, 52), 'grp-2': rect(600, 0, 600, 52) },
      () => null
    );

    expect(measureStripBands(document)).toEqual([
      { x: 0, y: 0, width: 600, height: 52 },
      { x: 600, y: 0, width: 600, height: 52 },
    ]);
  });

  it('drops zero-sized bands rather than registering rectangles that can never be hit', () => {
    document.body.innerHTML = `<div data-tab-strip-group="grp-1"></div>`;
    restore = withLayout({ 'grp-1': rect(0, 0, 0, 0) }, () => null);
    expect(measureStripBands(document)).toEqual([]);
  });

  it('answers with an empty list when this window has no strips at all', () => {
    restore = withLayout({}, () => null);
    expect(measureStripBands(document)).toEqual([]);
  });
});

describe('resolveMergeInsertion', () => {
  function stripWithTabs() {
    document.body.innerHTML = `
      <div data-tab-strip-group="grp-1">
        <div data-tab-id="tab-a"></div>
        <div data-tab-id="tab-b"></div>
        <div data-tab-id="tab-c"></div>
      </div>
    `;
    return document.querySelector<HTMLElement>('[data-tab-strip-group]')!;
  }

  const TAB_RECTS = {
    'tab-a': rect(0, 0, 100, 34),
    'tab-b': rect(100, 0, 100, 34),
    'tab-c': rect(200, 0, 100, 34),
    'grp-1': rect(0, 0, 300, 34),
  };

  it('lands before the first tab when the cursor is left of its midpoint', () => {
    const strip = stripWithTabs();
    restore = withLayout(TAB_RECTS, () => strip);
    expect(resolveMergeInsertion(document, 10, 10)).toEqual({
      groupId: 'grp-1',
      index: 0,
      beforeTabId: 'tab-a',
    });
  });

  it('steps one slot at a time as the cursor crosses each midpoint', () => {
    const strip = stripWithTabs();
    restore = withLayout(TAB_RECTS, () => strip);
    expect(resolveMergeInsertion(document, 60, 10)?.index).toBe(1);
    expect(resolveMergeInsertion(document, 160, 10)?.index).toBe(2);
    expect(resolveMergeInsertion(document, 260, 10)?.index).toBe(3);
  });

  it('appends past the last tab, with no tab to hang the caret on', () => {
    const strip = stripWithTabs();
    restore = withLayout(TAB_RECTS, () => strip);
    expect(resolveMergeInsertion(document, 400, 10)).toEqual({
      groupId: 'grp-1',
      index: 3,
      beforeTabId: null,
    });
  });

  it('resolves an EMPTY strip to index 0 — a group with no tabs is a valid target', () => {
    document.body.innerHTML = `<div data-tab-strip-group="grp-empty"></div>`;
    const strip = document.querySelector<HTMLElement>('[data-tab-strip-group]')!;
    restore = withLayout({ 'grp-empty': rect(0, 0, 300, 34) }, () => strip);
    expect(resolveMergeInsertion(document, 50, 10)).toEqual({
      groupId: 'grp-empty',
      index: 0,
      beforeTabId: null,
    });
  });

  it('refuses a point that is over no strip — the caller must not invent a target', () => {
    // This is the answer that makes a failed merge safe: the target acknowledges
    // FALSE and the source keeps its tab.
    stripWithTabs();
    restore = withLayout(TAB_RECTS, () => document.body);
    expect(resolveMergeInsertion(document, 10, 400)).toBeNull();
  });

  it('survives a jsdom with no elementFromPoint rather than throwing', () => {
    // The whole gesture runs through this function; a missing DOM method must
    // degrade to "no target", not take the renderer down.
    expect(resolveMergeInsertion(document, 10, 10)).toBeNull();
  });
});

describe('countWindowTabs (D5 — the backstop)', () => {
  const state = (groups: Record<string, string[]>, layout: ChatGroupsState['layout']) =>
    ({
      version: 1,
      layout,
      groups: Object.fromEntries(
        Object.entries(groups).map(([groupId, tabIds]) => [
          groupId,
          {
            groupId,
            activeTabId: tabIds[0] ?? null,
            tabs: tabIds.map((tabId) => ({
              tabId,
              sessionId: `s-${tabId}`,
              title: tabId,
              userSetName: false,
            })),
          },
        ])
      ),
      activeGroupId: Object.keys(groups)[0],
      seq: 0,
    }) as unknown as ChatGroupsState;

  it('counts a single group', () => {
    expect(countWindowTabs(state({ a: ['t1', 't2'] }, { kind: 'leaf', groupId: 'a' }))).toBe(2);
  });

  it('counts ACROSS a split — two panes of one tab each is two tabs, not one', () => {
    // The distinction that matters: tearing either one out leaves a window with
    // something in it, so neither is the "only tab" D5 protects.
    const layout = {
      kind: 'branch',
      dir: 'row',
      sizes: [0.5, 0.5],
      children: [
        { kind: 'leaf', groupId: 'a' },
        { kind: 'leaf', groupId: 'b' },
      ],
    } as ChatGroupsState['layout'];
    expect(countWindowTabs(state({ a: ['t1'], b: ['t2'] }, layout))).toBe(2);
  });

  it('reports 1 for the window D5 refuses to empty', () => {
    expect(countWindowTabs(state({ a: ['t1'] }, { kind: 'leaf', groupId: 'a' }))).toBe(1);
  });

  it('reports 0 for an empty group without counting a phantom tab', () => {
    expect(countWindowTabs(state({ a: [] }, { kind: 'leaf', groupId: 'a' }))).toBe(0);
  });
});

describe('clampGhostToViewport (obligation 4)', () => {
  const GHOST = { width: 160, height: 34 };
  const VIEWPORT = { width: 1000, height: 800 };

  it('leaves a ghost inside the frame exactly where the drag put it', () => {
    expect(clampGhostToViewport({ x: 400, y: 300 }, GHOST, VIEWPORT)).toEqual({ x: 400, y: 300 });
  });

  it('pins a ghost dragged past the right edge, whole, against that edge', () => {
    // Not `x = viewport.width`: the ghost's own width has to stay on screen or
    // the clamp only moves where it gets clipped.
    expect(clampGhostToViewport({ x: 1400, y: 300 }, GHOST, VIEWPORT)).toEqual({ x: 840, y: 300 });
  });

  it('pins past the left and top edges too', () => {
    expect(clampGhostToViewport({ x: -300, y: -80 }, GHOST, VIEWPORT)).toEqual({ x: 0, y: 0 });
  });

  it('clamps both axes at once, for a drag off a corner', () => {
    expect(clampGhostToViewport({ x: 5000, y: 5000 }, GHOST, VIEWPORT)).toEqual({
      x: 840,
      y: 766,
    });
  });

  it('passes the point through when the ghost has not been measured yet', () => {
    // First frame, or jsdom. Moving a ghost the user can already see, because we
    // have not measured it, would be a visible jump caused by our own ignorance.
    expect(clampGhostToViewport({ x: 4000, y: 40 }, { width: 0, height: 0 }, VIEWPORT)).toEqual({
      x: 4000,
      y: 40,
    });
  });

  it('passes the point through when the viewport has no size', () => {
    expect(clampGhostToViewport({ x: 40, y: 40 }, GHOST, { width: 0, height: 0 })).toEqual({
      x: 40,
      y: 40,
    });
  });

  it('does not push a ghost wider than the window off its left edge', () => {
    // A window narrower than one tab is degenerate but reachable (minWidth is
    // 800, a tab caps at 190 — so this is really the guard against a future
    // narrower floor). Clamping to a negative max would hide the label.
    expect(
      clampGhostToViewport(
        { x: 50, y: 10 },
        { width: 400, height: 34 },
        {
          width: 300,
          height: 800,
        }
      )
    ).toEqual({ x: 0, y: 10 });
  });
});
