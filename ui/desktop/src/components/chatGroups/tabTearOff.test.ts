import { describe, it, expect } from 'vitest';
import { isOutsideViewport, payloadFromTab, insertionIndexFromStrip } from './tabTearOff';
import { ChatTab } from './chatGroupsTypes';

// A window whose content rect is 1200x800. Deliberately not square, for the same
// reason dropZones.test.ts uses a 400x200 box: an implementation that compared x
// against the height would still pass on a square.
const VIEWPORT = { width: 1200, height: 800 };

describe('isOutsideViewport', () => {
  it('calls a point in the middle of the window inside', () => {
    expect(isOutsideViewport({ x: 600, y: 400 }, VIEWPORT)).toBe(false);
  });

  it('is inclusive at all four edges — the edge is still the window', () => {
    expect(isOutsideViewport({ x: 0, y: 400 }, VIEWPORT)).toBe(false);
    expect(isOutsideViewport({ x: 1200, y: 400 }, VIEWPORT)).toBe(false);
    expect(isOutsideViewport({ x: 600, y: 0 }, VIEWPORT)).toBe(false);
    expect(isOutsideViewport({ x: 600, y: 800 }, VIEWPORT)).toBe(false);
  });

  it('calls a point one pixel past any edge outside', () => {
    expect(isOutsideViewport({ x: -1, y: 400 }, VIEWPORT)).toBe(true);
    expect(isOutsideViewport({ x: 1201, y: 400 }, VIEWPORT)).toBe(true);
    expect(isOutsideViewport({ x: 600, y: -1 }, VIEWPORT)).toBe(true);
    expect(isOutsideViewport({ x: 600, y: 801 }, VIEWPORT)).toBe(true);
  });

  it('does not confuse the axes', () => {
    // Inside vertically, far past the right edge. An implementation that tested
    // x against `height` would call this inside (900 < ... no) — and the mirror
    // case below is the one that catches the swap: y=900 is past the height but
    // well within the width.
    expect(isOutsideViewport({ x: 900, y: 900 }, VIEWPORT)).toBe(true);
    expect(isOutsideViewport({ x: 900, y: 700 }, VIEWPORT)).toBe(false);
  });

  it('fails CLOSED on a non-finite point: unusable coordinates stay local', () => {
    // NaN compares false in every direction, so a naive implementation returns
    // "inside" here too — by accident. Assert it on purpose, because the safe
    // answer is the one that can only ever reorder a tab, never spawn a window.
    expect(isOutsideViewport({ x: Number.NaN, y: 400 }, VIEWPORT)).toBe(false);
    expect(isOutsideViewport({ x: 600, y: Number.NaN }, VIEWPORT)).toBe(false);
    expect(isOutsideViewport({ x: Number.POSITIVE_INFINITY, y: 400 }, VIEWPORT)).toBe(false);
  });

  it('fails CLOSED on a degenerate viewport (jsdom before layout, a minimising window)', () => {
    expect(isOutsideViewport({ x: 600, y: 400 }, { width: 0, height: 0 })).toBe(false);
    expect(isOutsideViewport({ x: 600, y: 400 }, { width: 1200, height: 0 })).toBe(false);
    expect(isOutsideViewport({ x: -50, y: -50 }, { width: 0, height: 800 })).toBe(false);
  });
});

describe('payloadFromTab', () => {
  const FULL: ChatTab = {
    tabId: 'tab-7',
    sessionId: 'sess-abc',
    title: 'Volcano plot',
    userSetName: true,
    workflowId: 'wf-2',
    cwd: '/Users/x/project',
    pendingInitialMessage: 'run the analysis',
    pendingInitialAttachments: [{ kind: 'image', path: '/tmp/a.png' }],
  };

  it('carries the fields that identify the chat', () => {
    expect(payloadFromTab(FULL)).toEqual({
      sessionId: 'sess-abc',
      title: 'Volcano plot',
      userSetName: true,
      workflowId: 'wf-2',
      cwd: '/Users/x/project',
    });
  });

  it('does NOT carry tabId — the receiving window mints its own', () => {
    // tabId is window-local identity (chatGroupsTypes.ts:6-10). Carried across,
    // it would collide with a tab the target window already has.
    expect('tabId' in payloadFromTab(FULL)).toBe(false);
  });

  it('does NOT carry the queued message or its attachments', () => {
    // The same data bug chatGroupsStorage.stripTransient refuses to persist,
    // arriving by a different door: a queued message that re-sends in a second
    // window sends the turn twice.
    const payload = payloadFromTab(FULL) as unknown as Record<string, unknown>;
    expect('pendingInitialMessage' in payload).toBe(false);
    expect('pendingInitialAttachments' in payload).toBe(false);
  });

  it('omits absent optional fields rather than sending them as undefined', () => {
    const bare: ChatTab = {
      tabId: 'tab-1',
      sessionId: 'sess-1',
      title: 'New Session',
      userSetName: false,
    };
    const payload = payloadFromTab(bare);
    expect(payload).toEqual({ sessionId: 'sess-1', title: 'New Session', userSetName: false });
    // `{cwd: undefined}` and `{}` are the same IPC message but not the same
    // object; assert the key is gone so a consumer's `'cwd' in payload` holds.
    expect(Object.keys(payload).sort()).toEqual(['sessionId', 'title', 'userSetName']);
  });

  it('is built by construction, so a new ChatTab field cannot leak by default', () => {
    // A field nobody has thought about yet, spread onto a tab. If payloadFromTab
    // ever becomes `{...tab}` minus a deny-list, this starts failing — which is
    // the point: the payload is an allow-list.
    const withFutureField = { ...FULL, someFutureSecret: 'do not travel' } as ChatTab;
    expect(Object.keys(payloadFromTab(withFutureField))).not.toContain('someFutureSecret');
  });
});

describe('insertionIndexFromStrip', () => {
  // Three 100px tabs laid end to end from x=40. Midpoints: 90, 190, 290.
  const RECTS = [
    { tabId: 'a', left: 40, width: 100 },
    { tabId: 'b', left: 140, width: 100 },
    { tabId: 'c', left: 240, width: 100 },
  ];

  it('inserts first when the cursor is left of everything', () => {
    expect(insertionIndexFromStrip(0, RECTS)).toBe(0);
    expect(insertionIndexFromStrip(-500, RECTS)).toBe(0);
    expect(insertionIndexFromStrip(89, RECTS)).toBe(0);
  });

  it('appends when the cursor is right of everything', () => {
    expect(insertionIndexFromStrip(340, RECTS)).toBe(3);
    expect(insertionIndexFromStrip(5000, RECTS)).toBe(3);
  });

  it('steps one index at a time across each midpoint', () => {
    expect(insertionIndexFromStrip(90, RECTS)).toBe(1); // exactly on a's midpoint
    expect(insertionIndexFromStrip(150, RECTS)).toBe(1);
    expect(insertionIndexFromStrip(190, RECTS)).toBe(2);
    expect(insertionIndexFromStrip(250, RECTS)).toBe(2);
    expect(insertionIndexFromStrip(290, RECTS)).toBe(3);
  });

  it('answers everywhere on the axis, including the gaps between tabs', () => {
    // Spaced tabs with 20px gutters: midpoints 50, 170, 290. A point in a gutter
    // has no tab under it, so an "which tab am I over?" implementation has no
    // answer here and the caret would vanish or stick.
    const spaced = [
      { tabId: 'a', left: 0, width: 100 },
      { tabId: 'b', left: 120, width: 100 },
      { tabId: 'c', left: 240, width: 100 },
    ];
    expect(insertionIndexFromStrip(110, spaced)).toBe(1);
    expect(insertionIndexFromStrip(230, spaced)).toBe(2);
  });

  it('is monotonic in clientX — the caret never runs backwards', () => {
    let previous = -1;
    for (let x = -20; x <= 400; x += 7) {
      const index = insertionIndexFromStrip(x, RECTS);
      expect(index).toBeGreaterThanOrEqual(previous);
      previous = index;
    }
    expect(previous).toBe(3);
  });

  it('returns 0 for an empty strip — the only index an empty strip has', () => {
    expect(insertionIndexFromStrip(0, [])).toBe(0);
    expect(insertionIndexFromStrip(999, [])).toBe(0);
  });

  it('stays ordered when every rect measures zero (jsdom computes no layout)', () => {
    const zeroed = [
      { tabId: 'a', left: 0, width: 0 },
      { tabId: 'b', left: 0, width: 0 },
    ];
    expect(insertionIndexFromStrip(-1, zeroed)).toBe(0);
    expect(insertionIndexFromStrip(0, zeroed)).toBe(2);
  });

  it('appends on a non-finite clientX rather than silently inserting first', () => {
    expect(insertionIndexFromStrip(Number.NaN, RECTS)).toBe(3);
  });
});
