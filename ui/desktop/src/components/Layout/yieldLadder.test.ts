import { describe, expect, it } from 'vitest';
import { GroupLayout } from '../chatGroups/chatGroupsTypes';
import {
  CHAT_MIN_WIDTH,
  PREVIEW_YIELD_WIDTH,
  SplitYieldAction,
  layoutFitsWidth,
  layoutMinWidth,
  previewPanelMode,
  shouldShowTabOverflowMenu,
  shouldCollapseComposerToolbar,
  COMPOSER_TOOLBAR_MIN_WIDTH,
  splitSnapshotIsStale,
  splitYieldAction,
  splitYieldFits,
  splitYieldSample,
} from './yieldLadder';

const leaf = (groupId: string): GroupLayout => ({ kind: 'leaf', groupId });
const row = (...children: GroupLayout[]): GroupLayout => ({
  kind: 'branch',
  dir: 'row',
  children,
  sizes: children.map(() => 1 / children.length),
});
const col = (...children: GroupLayout[]): GroupLayout => ({
  kind: 'branch',
  dir: 'col',
  children,
  sizes: children.map(() => 1 / children.length),
});

/**
 * Rung 2. The bug this encodes was MEASURED in the real app: a 2-up split in a
 * 1000px window gave each pane ~500px; the preview panel took its 360px floor
 * and left the transcript 140px. The old rule asked the WINDOW (isMobile, <930)
 * whether there was room, and the window said yes.
 */
describe('previewPanelMode (rung 2 — the preview panel yields its column)', () => {
  it('keeps its column when the pane can seat both floors', () => {
    expect(previewPanelMode({ isMobile: false, paneWidth: 1200 })).toBe('side');
    expect(previewPanelMode({ isMobile: false, paneWidth: PREVIEW_YIELD_WIDTH })).toBe('side');
  });

  it('yields its column one pixel below the two floors', () => {
    expect(previewPanelMode({ isMobile: false, paneWidth: PREVIEW_YIELD_WIDTH - 1 })).toBe(
      'overlay'
    );
  });

  it('THE REGRESSION: a 500px pane in a roomy window yields instead of starving the chat', () => {
    // 2-up split, 1000px window. Window-based isMobile says "not mobile", so the
    // old rule kept the panel in its column and the transcript got 140px.
    expect(previewPanelMode({ isMobile: false, paneWidth: 500 })).toBe('overlay');
  });

  it('leaves the shipped mobile overlay exactly as it was', () => {
    expect(previewPanelMode({ isMobile: true, paneWidth: 1600 })).toBe('overlay');
  });

  it('does not slam into an overlay on an unmeasured pane', () => {
    for (const paneWidth of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(previewPanelMode({ isMobile: false, paneWidth })).toBe('side');
    }
  });

  it('is pure: the same pane width always gives the same answer', () => {
    // The rung has no memory ON PURPOSE — nothing to fight the user with. Ten
    // samples at one width must not drift the way a stateful rule could.
    const answers = new Set(
      Array.from({ length: 10 }, () => previewPanelMode({ isMobile: false, paneWidth: 640 }))
    );
    expect([...answers]).toEqual(['overlay']);
  });
});

describe('shouldCollapseComposerToolbar (rung 3b — the composer collapses to a +)', () => {
  it('collapses once the row is narrower than the pickers need', () => {
    expect(shouldCollapseComposerToolbar({ availableWidth: COMPOSER_TOOLBAR_MIN_WIDTH - 1 })).toBe(
      true
    );
  });

  it('stays expanded at and above the threshold', () => {
    expect(shouldCollapseComposerToolbar({ availableWidth: COMPOSER_TOOLBAR_MIN_WIDTH })).toBe(
      false
    );
    expect(shouldCollapseComposerToolbar({ availableWidth: 1200 })).toBe(false);
  });

  it('does not flash collapsed on an unmeasured (0/NaN) box at first paint', () => {
    expect(shouldCollapseComposerToolbar({ availableWidth: 0 })).toBe(false);
    expect(shouldCollapseComposerToolbar({ availableWidth: NaN })).toBe(false);
  });

  it('is monotone: measuring the row’s OWN box, not its content, so it cannot oscillate', () => {
    // The collapsed state removes controls, changing content width — but this
    // rule reads the container width, which the pane fixes regardless. So for a
    // given width the decision is the same whether currently collapsed or not.
    for (let w = 200; w <= 900; w += 13) {
      const decision = shouldCollapseComposerToolbar({ availableWidth: w });
      // Re-measuring the same box yields the same decision — no dependence on
      // the current collapsed/expanded content.
      expect(shouldCollapseComposerToolbar({ availableWidth: w })).toBe(decision);
    }
  });
});

describe('shouldShowTabOverflowMenu (rung 3 — shrink, scroll, then ▾, never wrap)', () => {
  it('shows the menu once tabs are scrolled out of sight', () => {
    expect(shouldShowTabOverflowMenu({ scrollWidth: 800, clientWidth: 300, tabCount: 6 })).toBe(
      true
    );
  });

  it('stays away while the tabs fit', () => {
    expect(shouldShowTabOverflowMenu({ scrollWidth: 300, clientWidth: 300, tabCount: 3 })).toBe(
      false
    );
  });

  it('ignores sub-pixel overflow', () => {
    expect(shouldShowTabOverflowMenu({ scrollWidth: 300.4, clientWidth: 300, tabCount: 3 })).toBe(
      false
    );
  });

  it('never offers a menu that could only list the tab you are on', () => {
    expect(shouldShowTabOverflowMenu({ scrollWidth: 900, clientWidth: 40, tabCount: 1 })).toBe(
      false
    );
    expect(shouldShowTabOverflowMenu({ scrollWidth: 900, clientWidth: 40, tabCount: 0 })).toBe(
      false
    );
  });

  it('cannot oscillate: the button lives outside the scroll box, so both directions are monotone', () => {
    // Showing the ▾ costs the strip BUTTON_W of clientWidth. Model both states at
    // the same window and assert the pair is never in disagreement — that is what
    // "no hysteresis needed" actually claims, and it is the claim a sticky
    // in-strip button would have failed.
    const BUTTON_W = 30;
    const CONTENT = 800;
    for (let strip = 0; strip <= 1200; strip += 7) {
      const shown = shouldShowTabOverflowMenu({
        scrollWidth: CONTENT,
        clientWidth: strip - BUTTON_W,
        tabCount: 6,
      });
      const hidden = shouldShowTabOverflowMenu({
        scrollWidth: CONTENT,
        clientWidth: strip,
        tabCount: 6,
      });
      // The only forbidden pair is "hidden says show, and shown says hide" —
      // i.e. each state's measurement demanding the other. Adding width can
      // never create overflow, so this must hold at every width.
      expect(hidden && !shown).toBe(false);
    }
  });
});

describe('layoutMinWidth (rung 4 — what the tree actually costs in width)', () => {
  it('a single group costs one chat floor', () => {
    expect(layoutMinWidth(leaf('a'))).toBe(CHAT_MIN_WIDTH);
  });

  it('a row of two costs both floors plus the splitter', () => {
    expect(layoutMinWidth(row(leaf('a'), leaf('b')))).toBe(721);
  });

  it('a row of four costs four floors plus three splitters', () => {
    expect(layoutMinWidth(row(leaf('a'), leaf('b'), leaf('c'), leaf('d')))).toBe(1443);
  });

  it('a COLUMN of two costs the width of ONE — stacked groups do not divide width', () => {
    // This is why it is a tree walk and not `groupCount * CHAT_MIN`. Counting
    // leaves would merge a perfectly usable vertical split at 700px.
    expect(layoutMinWidth(col(leaf('a'), leaf('b')))).toBe(CHAT_MIN_WIDTH);
    expect(layoutFitsWidth(col(leaf('a'), leaf('b')), 700)).toBe(true);
  });

  it('a column of rows costs the widest row', () => {
    expect(layoutMinWidth(col(row(leaf('a'), leaf('b')), leaf('c')))).toBe(721);
  });

  it('nests: a row whose child is a row', () => {
    expect(layoutMinWidth(row(leaf('a'), row(leaf('b'), leaf('c'))))).toBe(360 + 1 + 721);
  });

  it('treats an unmeasured width as fitting rather than merging a split nobody saw', () => {
    for (const width of [0, -5, Number.NaN]) {
      expect(layoutFitsWidth(row(leaf('a'), leaf('b')), width)).toBe(true);
    }
  });
});

describe('splitYieldFits (which layout the fit is judged against)', () => {
  const split = row(leaf('a'), leaf('b'));

  it('judges the live layout when we hold no snapshot', () => {
    expect(splitYieldFits({ layout: split, snapshotLayout: null, availableWidth: 900 })).toBe(true);
    expect(splitYieldFits({ layout: split, snapshotLayout: null, availableWidth: 700 })).toBe(false);
  });

  it('THE TRAP: while merged, judges the layout we OWE, not the merged leaf', () => {
    // After a merge the live layout is one leaf, which fits at every width. Ask
    // it and the next shrink-step reads as a crossing back INTO fitting — and
    // the window re-splits at 500px, which is the sidebar bug in a new costume.
    expect(
      splitYieldFits({ layout: leaf('a'), snapshotLayout: split, availableWidth: 500 })
    ).toBe(false);
    expect(
      splitYieldFits({ layout: leaf('a'), snapshotLayout: split, availableWidth: 900 })
    ).toBe(true);
  });
});

describe('splitSnapshotIsStale', () => {
  it('is ours to keep while the merged layout is still one group', () => {
    expect(splitSnapshotIsStale({ groupCount: 1 })).toBe(false);
  });

  it('is forfeit the moment the user splits again by hand', () => {
    expect(splitSnapshotIsStale({ groupCount: 2 })).toBe(true);
  });
});

/**
 * The watcher, modelled exactly as the shell runs it. Every rung-4 sequence test
 * drives THIS, so the composition under test cannot drift from the composition
 * that ships.
 */
function makeWatcher(initial: GroupLayout) {
  let layout = initial;
  let snapshot: GroupLayout | null = null;
  let lastWidth: number | null = null;

  const sample = (width: number): SplitYieldAction => {
    const groupCount = layout.kind === 'leaf' ? 1 : layout.children.length;
    if (snapshot && splitSnapshotIsStale({ groupCount })) snapshot = null;
    const { wasFitting, isFitting } = splitYieldSample({
      layout,
      snapshotLayout: snapshot,
      lastWidth,
      width,
    });
    const action = splitYieldAction({
      wasFitting,
      isFitting,
      groupCount,
      autoMerged: snapshot !== null,
    });
    lastWidth = width;
    if (action === 'merge') {
      snapshot = layout;
      layout = leaf('a');
    } else if (action === 'restore') {
      layout = snapshot!;
      snapshot = null;
    }
    return action;
  };

  return {
    sample,
    split: (next: GroupLayout) => {
      layout = next;
    },
    get layout() {
      return layout;
    },
    get snapshot() {
      return snapshot;
    },
  };
}

describe('splitYieldSample (the crossing is on WIDTH, never on the layout)', () => {
  it('THE REGRESSION: splitting by hand at a stable width is not a crossing', () => {
    // A watcher that cached `wasFitting` as a bare boolean would say
    // was=true (one leaf fits 600) / is=false (two leaves need 721) and merge
    // the user's split inside the same tick as their drop — the sidebar bug,
    // rebuilt. Re-deriving the previous side from the previous WIDTH makes the
    // two sides agree, because the width did not move.
    const before = splitYieldSample({
      layout: row(leaf('a'), leaf('b')),
      snapshotLayout: null,
      lastWidth: 600,
      width: 600,
    });
    expect(before.wasFitting).toBe(before.isFitting);
    expect(splitYieldAction({ ...before, groupCount: 2, autoMerged: false })).toBe('none');
  });

  it('reports no previous side on the first sample', () => {
    expect(
      splitYieldSample({ layout: leaf('a'), snapshotLayout: null, lastWidth: null, width: 500 })
        .wasFitting
    ).toBeNull();
  });
});

describe('splitYieldAction (rung 4 — merge rather than render two useless slivers)', () => {
  it('replays a full shrink/grow sweep without ever fighting the user', () => {
    // The sidebar's regression test, ported to the split — because this is the
    // same effect shape and would fail the same way.
    const w = makeWatcher(row(leaf('a'), leaf('b'))); // user splits 2-up

    expect(w.sample(1400)).toBe('none'); // wide, split stands
    expect(w.sample(900)).toBe('none'); // still fits (721)
    expect(w.sample(700)).toBe('merge'); // crossed the floor
    expect(w.layout).toEqual(leaf('a'));
    expect(w.sample(700)).toBe('none'); // the watcher re-runs after the merge: no-op
    expect(w.sample(500)).toBe('none'); // shrink further: judged against the snapshot
    expect(w.layout).toEqual(leaf('a'));
    expect(w.sample(900)).toBe('restore'); // back over the floor: the split returns
    expect(w.layout).toEqual(row(leaf('a'), leaf('b')));
    expect(w.sample(900)).toBe('none'); // and settles
    expect(w.snapshot).toBeNull();
  });

  it('keeps a split the user made BY HAND in a too-narrow window', () => {
    // The user wins. No width crossing happened, so the ladder is silent — and
    // stays silent however many times the watcher re-runs.
    const w = makeWatcher(leaf('a'));
    expect(w.sample(600)).toBe('none');
    w.split(row(leaf('a'), leaf('b'))); // the user splits anyway, at 600px
    expect(w.sample(600)).toBe('none');
    expect(w.sample(600)).toBe('none');
    expect(w.layout.kind).toBe('branch'); // their split survives
  });

  it('forgets the snapshot when the user re-splits while merged, and never resurrects it', () => {
    const w = makeWatcher(row(leaf('a'), leaf('b')));
    expect(w.sample(1400)).toBe('none');
    expect(w.sample(700)).toBe('merge');
    w.split(col(leaf('a'), leaf('c'))); // the user builds their OWN layout while merged
    expect(w.sample(700)).toBe('none'); // the snapshot is forfeit here
    expect(w.snapshot).toBeNull();
    expect(w.sample(1400)).toBe('none'); // growing back must NOT restore the old split
    expect(w.layout).toEqual(col(leaf('a'), leaf('c'))); // the user's layout stands
  });

  it('merges a persisted split that loads into an already-too-narrow window', () => {
    const w = makeWatcher(row(leaf('a'), leaf('b')));
    expect(w.sample(700)).toBe('merge'); // first sample, no previous side
    expect(w.sample(1400)).toBe('restore'); // and it is still owed back
  });

  it('does nothing when the width did not cross the threshold', () => {
    expect(
      splitYieldAction({ wasFitting: false, isFitting: false, groupCount: 2, autoMerged: false })
    ).toBe('none');
  });

  it('never fights the user at a stable width, in any state combination', () => {
    for (const fitting of [true, false]) {
      for (const groupCount of [1, 2, 4]) {
        for (const autoMerged of [true, false]) {
          expect(
            splitYieldAction({
              wasFitting: fitting,
              isFitting: fitting,
              groupCount,
              autoMerged,
            })
          ).toBe('none');
        }
      }
    }
  });

  it('merges on crossing INTO too-narrow while there is a split', () => {
    expect(
      splitYieldAction({ wasFitting: true, isFitting: false, groupCount: 2, autoMerged: false })
    ).toBe('merge');
  });

  it('merges on the very first sample in a window that is already too narrow', () => {
    expect(
      splitYieldAction({ wasFitting: null, isFitting: false, groupCount: 3, autoMerged: false })
    ).toBe('merge');
  });

  it('has nothing to merge when there is only one group', () => {
    expect(
      splitYieldAction({ wasFitting: true, isFitting: false, groupCount: 1, autoMerged: false })
    ).toBe('none');
  });

  it('restores on crossing OUT only if WE merged', () => {
    expect(
      splitYieldAction({ wasFitting: false, isFitting: true, groupCount: 1, autoMerged: true })
    ).toBe('restore');
    expect(
      splitYieldAction({ wasFitting: false, isFitting: true, groupCount: 1, autoMerged: false })
    ).toBe('none');
  });
});
