import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import { ChatTabStrip, TAB_BAND_DRAG_GUTTER } from './ChatTabStrip';
import type { ChatTab } from './chatGroupsTypes';

/**
 * The tab strip's `-webkit-app-region` set must not depend on the tab list.
 *
 * WHAT THIS FILE CAN AND CANNOT PROVE. jsdom has no layout engine and no
 * concept of app regions, so nothing here can fail on the OS behaviour itself:
 * a synthetic press reaches the tab whether or not macOS would have eaten it,
 * and the strip's own suite passed throughout the lifetime of the bug below.
 * What jsdom CAN see is the structural invariant the fix rests on, which is
 * readable from inline styles alone — that the declarations are the same for
 * one tab and for many. The gate that can see the rest is
 * `scripts/titlebar-appregion-check.mjs` against a running app (it now samples
 * the strip band as well as the titlebar controls), plus a real CGEventPost
 * sweep.
 *
 * THE BUG. The strip used to be `-webkit-app-region: drag` with a `no-drag` on
 * each tab, so the region set changed every time a tab was opened, closed, torn
 * off or merged in. Blink recollects those rects during a paint lifecycle and
 * ships them to the browser process over IPC; until that lands, macOS routes
 * with the PREVIOUS set — so a brand-new tab sat inside the strip's stale
 * `drag` rect and a press on it produced no `pointerdown` at all. The OS read
 * it as a titlebar grab and moved the WINDOW. Measured against the running app
 * by sweeping a real cursor across the second tab's x-range (Electron's
 * DragRegionView swallows every event over a `drag` rect, so an arrival is a
 * read-out of the region set): 0/11 sample points arrived with one tab open,
 * 11/11 with two, and 1/11 in the 2.5s after a tab was created while the
 * renderer was busy — with the DOM holding the tab there the whole time.
 *
 * So the invariant is not "every tab declares no-drag" — that was the broken
 * design, and an earlier test in ChatTabStrip.test.tsx enforced it. It is that
 * the strip declares ONE no-drag rect, over the whole scroll box, identical for
 * every tab count, with nothing inside it declaring `drag`.
 */

const REGION = 'WebkitAppRegion';

type RegionStyle = CSSStyleDeclaration & { WebkitAppRegion?: string };

/** Every app-region declaration in tree order — the order Electron folds in. */
function regions(container: HTMLElement): Array<{ label: string; mode: string }> {
  return Array.from(container.querySelectorAll<HTMLElement>('*'))
    .map((el) => ({ el, mode: (el.style as RegionStyle)[REGION] }))
    .filter((entry): entry is { el: HTMLElement; mode: string } => !!entry.mode)
    .map(({ el, mode }) => ({
      label:
        el.getAttribute('data-testid') ||
        el.getAttribute('data-tab-id') ||
        (typeof el.className === 'string' && el.className.trim().split(/\s+/)[0]) ||
        el.tagName.toLowerCase(),
      mode,
    }));
}

function tabs(count: number): ChatTab[] {
  return Array.from({ length: count }, (_, i) => ({
    tabId: `tab-${i + 1}`,
    sessionId: `s${i + 1}`,
    title: `Chat ${i + 1}`,
    userSetName: false,
  }));
}

function renderStrip(count: number) {
  const list = tabs(count);
  return render(
    <ChatTabStrip
      tabs={list}
      activeTabId={list[0]?.tabId ?? null}
      runningSessionIds={[]}
      onSelect={vi.fn()}
      onClose={vi.fn()}
      onReorder={vi.fn()}
      reserveTitlebar={false}
      isCompactSidebarOverlayOpen={false}
      endSlot={<button type="button">+</button>}
    />
  );
}

describe('ChatTabStrip — the strip’s app-region set is static (the tab-creation race)', () => {
  it('declares exactly the same regions for 0, 1, 2 and 8 tabs', () => {
    // THE ASSERTION THAT MATTERS. Opening a tab must add no rect and move no
    // rect, because every rect added or moved is one the browser process only
    // learns about a lifecycle later. Restore the per-tab `no-drag` and this
    // list grows with the tab count.
    const baseline = (() => {
      const { container, unmount } = renderStrip(1);
      const list = regions(container);
      unmount();
      return list;
    })();

    for (const count of [0, 2, 8]) {
      const { container, unmount } = renderStrip(count);
      expect(regions(container), `tab count ${count}`).toEqual(baseline);
      unmount();
    }
  });

  it('the one rect covering the tabs is the scroll box, and it is no-drag', () => {
    const { getByTestId } = renderStrip(3);
    expect((getByTestId('chat-tab-strip').style as RegionStyle)[REGION]).toBe('no-drag');
  });

  it('nothing inside the scroll box declares drag (it would fold later and re-cover the tabs)', () => {
    // #74 in miniature: Electron folds in tree order, so any `drag` rect
    // declared under the strip wins over the strip's own `no-drag` and puts the
    // tabs back inside a draggable region — with no test failing anywhere else.
    const { getByTestId } = renderStrip(4);
    const inside = regions(getByTestId('chat-tab-strip'));
    expect(inside.filter((r) => r.mode === 'drag')).toEqual([]);
  });

  it('no tab and no tab child declares an app region at all', () => {
    const { container } = renderStrip(4);
    for (const tab of Array.from(container.querySelectorAll<HTMLElement>('[data-tab-id]'))) {
      for (const el of [tab, ...Array.from(tab.querySelectorAll<HTMLElement>('*'))]) {
        expect((el.style as RegionStyle)[REGION]).toBeFalsy();
      }
    }
  });

  it('keeps a static window-drag handle at the right end of the band', () => {
    // The scroll box is no-drag edge to edge, so without this the tab band
    // stops moving the window entirely. It has to be padding on the WRAP — the
    // one box in this row whose width owes nothing to the tab list. A gutter
    // sized off the tabs would be the same race wearing a different hat.
    for (const count of [0, 1, 8]) {
      const { getByTestId, unmount } = renderStrip(count);
      const wrap = getByTestId('chat-tab-strip-reserve');
      expect((wrap.style as RegionStyle)[REGION]).toBe('drag');
      expect(wrap.style.paddingRight).toBe(`${TAB_BAND_DRAG_GUTTER}px`);
      unmount();
    }
  });
});
