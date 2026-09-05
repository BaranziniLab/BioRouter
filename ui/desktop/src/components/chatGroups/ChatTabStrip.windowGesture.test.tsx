import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatTabStrip, ChatTabStripProps } from './ChatTabStrip';
import type { ChatTab } from './chatGroupsTypes';

/**
 * The empty part of the tab band is a titlebar: drag moves the window,
 * double-click zooms it.
 *
 * WHAT THIS FILE CAN AND CANNOT PROVE. jsdom has no layout engine, so it cannot
 * tell you where the empty area IS, and it has no notion of `-webkit-app-region`
 * — the reason this gesture is built from pointer events in the first place. Two
 * sibling gates own the parts jsdom cannot see: `ChatTabStrip.appRegion.test.tsx`
 * pins that this change adds no app-region rect (the whole point — a `drag`
 * spacer sized off the leftover space would re-create the tab-creation race it
 * documents), and `scripts/titlebar-appregion-check.mjs` reads the real fold out
 * of a running app.
 *
 * What IS checkable here is the routing, which is the half that decides whether
 * the gesture fires on the right thing: event delegation from the strip's
 * children, the primary-button guard, and — the one with teeth — that every
 * path out of a press sends main its end message. Main drives the window from a
 * timer on its own side and cannot see the mouse button come up, so a missing
 * end is a window that keeps following the cursor.
 */

const bridge = vi.hoisted(() => ({
  windowDragStart: vi.fn(),
  windowDragEnd: vi.fn(),
  windowToggleZoom: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, { electron: { ...bridge } });
});

function tabs(count: number): ChatTab[] {
  return Array.from({ length: count }, (_, i) => ({
    tabId: `tab-${i + 1}`,
    sessionId: `2026090${i + 1}_1`,
    title: `Chat ${i + 1}`,
    userSetName: false,
  }));
}

function renderStrip(over: Partial<ChatTabStripProps> = {}) {
  const list = over.tabs ?? tabs(2);
  const props: ChatTabStripProps = {
    tabs: list,
    activeTabId: list[0]?.tabId ?? null,
    runningSessionIds: [],
    onSelect: vi.fn(),
    onClose: vi.fn(),
    onReorder: vi.fn(),
    reserveTitlebar: false,
    isCompactSidebarOverlayOpen: false,
    endSlot: (
      <button type="button" data-testid="new-tab">
        +
      </button>
    ),
    ...over,
  };
  return { ...render(<ChatTabStrip {...props} />), props };
}

/** The scroll box — the element whose own background IS the empty band. */
function band() {
  return screen.getByTestId('chat-tab-strip');
}

describe('ChatTabStrip — the empty band drags the window', () => {
  it('starts a window drag on a press with nothing under it', () => {
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    expect(bridge.windowDragStart).toHaveBeenCalledTimes(1);
  });

  it('does NOT start one from a tab, its close control, or the end slot', () => {
    // The guard is `target === currentTarget`, so this is the assertion that the
    // gesture is delegated correctly: all three of these bubble THROUGH the
    // strip, and a naive handler on the strip would grab the window on every
    // tab click — which is precisely the bug the app-region version had.
    renderStrip();
    fireEvent.pointerDown(screen.getByRole('tab', { name: /Chat 2/ }), {
      button: 0,
      pointerId: 1,
    });
    fireEvent.pointerDown(screen.getByTestId('chat-tab-close-tab-1'), { button: 0, pointerId: 2 });
    fireEvent.pointerDown(screen.getByTestId('new-tab'), { button: 0, pointerId: 3 });
    expect(bridge.windowDragStart).not.toHaveBeenCalled();
  });

  it('ignores a right-click and a middle-click', () => {
    // A right-click on the band must stay available for a context menu, and a
    // middle-click must never move the window.
    renderStrip();
    fireEvent.pointerDown(band(), { button: 2, pointerId: 1 });
    fireEvent.pointerDown(band(), { button: 1, pointerId: 2 });
    expect(bridge.windowDragStart).not.toHaveBeenCalled();
  });

  it('starts a drag on the second press of a double-click too, and that is correct', () => {
    // ⚠ THIS ASSERTS THE ABSENCE OF THE OBVIOUS GUARD, on purpose. A
    // `detail > 1` check reads like the right way to let the zoom gesture past,
    // and it is dead code: `detail` is 0 on EVERY pointer event per spec, and
    // both presses of a real double-click were measured arriving as `detail: 0`
    // from CGEventPost input against the running app. Such a guard would fire
    // only under synthetic events — here and in CDP — which is the worst place
    // for a guard to work.
    //
    // What actually keeps a double-click from nudging the window is main's
    // movement threshold (a stationary press moves nothing) plus the ordering
    // in the zoom test below. Pin it here so nobody "fixes" the missing guard.
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.pointerUp(band(), { pointerId: 1 });
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    expect(bridge.windowDragStart).toHaveBeenCalledTimes(2);
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
  });

  it('ends the drag on pointerup', () => {
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.pointerUp(band(), { pointerId: 1 });
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
  });

  it('ends the drag on a window-level pointerup that never reached the element', () => {
    // THE ONE THAT MATTERS. Pointer capture makes the element's own `pointerup`
    // the ordinary path, but a capture that was never granted — or was revoked
    // when the strip re-rendered mid-gesture — would leave main's timer running
    // against a button that is already up, and main cannot see the button. The
    // capture-phase window listener is what cannot be lost that way.
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.pointerUp(document.body, { pointerId: 1 });
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
  });

  it('ends the drag on pointercancel and on the window losing focus', () => {
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.pointerCancel(band(), { pointerId: 1 });
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);

    fireEvent.pointerDown(band(), { button: 0, pointerId: 2, detail: 0 });
    fireEvent.blur(window);
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(2);
  });

  it('ends the drag when the strip unmounts mid-press', () => {
    const { unmount } = renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    unmount();
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
  });

  it('sends exactly one end however many paths fire', () => {
    // Every redundant path lands on a drag that may already be over, so the end
    // has to be idempotent on this side too — main's is as well, but a renderer
    // that spams the channel would end a LATER drag if the two ever interleave.
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.pointerUp(band(), { pointerId: 1 });
    fireEvent.lostPointerCapture(band(), { pointerId: 1 });
    fireEvent.pointerCancel(band(), { pointerId: 1 });
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
  });

  it('detaches the first press before a second one takes its slot', () => {
    // `detachRef` is a SINGLE SLOT. A second press arriving with a drag still
    // open — the second half of a double-click, or a press whose release was
    // swallowed — used to overwrite the only handle to the previous window
    // listeners, leaving them attached for the life of the strip. A leaked
    // capture-phase listener is silent (its `endDrag` finds nothing to end), so
    // the balance of add/remove is the only thing that can see it.
    renderStrip();
    const addSpy = vi.spyOn(window, 'addEventListener');
    const removeSpy = vi.spyOn(window, 'removeEventListener');
    try {
      fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
      fireEvent.pointerDown(band(), { button: 0, pointerId: 2, detail: 0 });
      fireEvent.pointerUp(document.body, { pointerId: 2 });

      const added = addSpy.mock.calls.filter(([type]) => type === 'pointerup').length;
      const removed = removeSpy.mock.calls.filter(([type]) => type === 'pointerup').length;
      expect(added).toBe(2);
      expect(removed).toBe(2);
    } finally {
      addSpy.mockRestore();
      removeSpy.mockRestore();
    }
    // And the superseded drag is ended on the wire too, before the new one
    // starts — main's `begin` supersedes as well, so the order is what matters.
    expect(bridge.windowDragStart).toHaveBeenCalledTimes(2);
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(2);
  });

  it('does not send an end for a press it declined to act on', () => {
    renderStrip();
    fireEvent.pointerDown(screen.getByRole('tab', { name: /Chat 2/ }), { button: 0, pointerId: 1 });
    fireEvent.pointerUp(band(), { pointerId: 1 });
    expect(bridge.windowDragEnd).not.toHaveBeenCalled();
  });
});

describe('ChatTabStrip — the empty band zooms on double-click', () => {
  it('zooms on a double-click with nothing under it', () => {
    renderStrip();
    fireEvent.doubleClick(band());
    expect(bridge.windowToggleZoom).toHaveBeenCalledTimes(1);
  });

  it('does NOT zoom from a double-click on a tab or the end slot', () => {
    // Double-clicking a tab label is a plausible way to try to rename it; it
    // must not resize the window as a side effect.
    renderStrip();
    fireEvent.doubleClick(screen.getByRole('tab', { name: /Chat 1/ }));
    fireEvent.doubleClick(screen.getByTestId('new-tab'));
    expect(bridge.windowToggleZoom).not.toHaveBeenCalled();
  });

  it('does NOT zoom when the second press was dragged', () => {
    // THE GESTURE THAT DID BOTH. The second press of a double-click starts a
    // drag (there is no usable `detail` to tell the two presses apart), so a
    // user who presses twice and drags the second press has MOVED the window —
    // and an unconditional zoom then resizes it and throws that position away.
    // `dblclick` carries the release position, so the press origin recorded on
    // `pointerdown` is what says whether this was a drag.
    renderStrip();
    fireEvent.pointerDown(band(), {
      button: 0,
      pointerId: 1,
      detail: 0,
      screenX: 400,
      screenY: 12,
    });
    fireEvent.pointerUp(band(), { pointerId: 1 });
    fireEvent.pointerDown(band(), {
      button: 0,
      pointerId: 1,
      detail: 0,
      screenX: 400,
      screenY: 12,
    });
    fireEvent.doubleClick(band(), { screenX: 540, screenY: 260 });
    expect(bridge.windowToggleZoom).not.toHaveBeenCalled();
    // The drag still owes main its end message whether or not the zoom happens.
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(2);
  });

  it('still zooms when the press only jittered inside the move threshold', () => {
    // The other half of the same rule, and the reason the threshold is main's
    // own: a press below it moved NOTHING, so there is no position to protect
    // and a hand-tremor double-click must still zoom.
    renderStrip();
    fireEvent.pointerDown(band(), {
      button: 0,
      pointerId: 1,
      detail: 0,
      screenX: 400,
      screenY: 12,
    });
    fireEvent.doubleClick(band(), { screenX: 402, screenY: 13 });
    expect(bridge.windowToggleZoom).toHaveBeenCalledTimes(1);
  });

  it('closes any open drag before it zooms', () => {
    // The press that opened the double-click also opened a drag. If the zoom
    // landed first, main's timer would keep re-positioning the window against
    // bounds the zoom had already replaced.
    renderStrip();
    fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
    fireEvent.doubleClick(band());
    expect(bridge.windowDragEnd).toHaveBeenCalledTimes(1);
    expect(bridge.windowToggleZoom).toHaveBeenCalledTimes(1);
  });
});

describe('ChatTabStrip — the band gesture off the desktop app', () => {
  it('is inert with no window.electron, and with a preload that predates it', () => {
    // The browser build (`biorouter serve`) and the artifact harness both run
    // without `window.electron`, and a packaged app can run a renderer against
    // an older preload that has the object but not these methods. Neither may
    // throw on every press in the band — and there is no window to move in the
    // first place.
    Object.assign(window, { electron: undefined });
    const { unmount } = renderStrip();
    expect(() => {
      fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
      fireEvent.pointerUp(band(), { pointerId: 1 });
      fireEvent.doubleClick(band());
    }).not.toThrow();
    unmount();

    Object.assign(window, { electron: { createChatWindow: vi.fn() } });
    renderStrip();
    expect(() => {
      fireEvent.pointerDown(band(), { button: 0, pointerId: 1, detail: 0 });
      fireEvent.doubleClick(band());
    }).not.toThrow();
  });
});
