import { describe, it, expect, vi, afterEach, beforeAll, afterAll } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { useTabDragReorder } from './useTabDragReorder';
import { CrossWindowPhase, ScreenPoint } from './tabTearOff';
import { DropTarget } from './dropZones';

/**
 * WHAT JSDOM CAN AND CANNOT SAY ABOUT THIS HOOK.
 *
 * Cannot: pointer capture (unimplemented), `elementFromPoint` (no layout, always
 * null), `getBoundingClientRect` (all zeroes), and therefore the whole
 * reorder/pane/split routing — that is browser-verified, as the hook's own header
 * has said since it shipped.
 *
 * Can, and this is the only reason this file exists: the cross-window BRANCHING.
 * Whether a callback fires, which one, with what phase, and — the constraint that
 * matters most — that with no cross-window callback supplied the hook does not
 * consult the pointer's position outside the window at all. Those are decisions,
 * not geometry, and a decision is testable wherever it is made.
 */

const VIEWPORT_W = window.innerWidth;
const VIEWPORT_H = window.innerHeight;

interface HarnessProps {
  onReorder?: (a: string, b: string) => void;
  onDropToGroup?: (tabId: string, target: DropTarget) => void;
  onCrossWindow?: (phase: CrossWindowPhase, point: ScreenPoint) => void;
  onCrossWindowCommit?: (phase: CrossWindowPhase, point: ScreenPoint) => void;
  isTabLocked?: (tabId: string) => boolean;
}

function Harness(props: HarnessProps) {
  const drag = useTabDragReorder({ onReorder: props.onReorder ?? (() => {}), ...props });
  return (
    <div className="br-tab">
      <button
        data-testid="tab"
        type="button"
        onPointerDown={(event) => drag.beginDrag(event, 'tab-1', 'Volcano plot', 'grp-1')}
      >
        Volcano plot
      </button>
      <span data-testid="phase">{drag.crossWindow.kind}</span>
      <span data-testid="dragging">{drag.draggedTabId ?? 'none'}</span>
      <span data-testid="ghost">{drag.ghost ? 'yes' : 'no'}</span>
    </div>
  );
}

/** Press, then move far enough to promote. Both points are inside the window. */
function pressAndPromote() {
  fireEvent.pointerDown(screen.getByTestId('tab'), {
    button: 0,
    pointerId: 1,
    clientX: 100,
    clientY: 20,
  });
  fireEvent.pointerMove(window, {
    pointerId: 1,
    clientX: 200,
    clientY: 20,
    screenX: 200,
    screenY: 120,
  });
}

/** A move to a point beyond the window's right edge. */
function moveOutside() {
  fireEvent.pointerMove(window, {
    pointerId: 1,
    clientX: VIEWPORT_W + 50,
    clientY: 20,
    screenX: VIEWPORT_W + 50,
    screenY: 120,
  });
}

afterEach(cleanup);

/**
 * This jsdom does not define `document.elementFromPoint` AT ALL — not "returns
 * null", missing, so the hook's move handler throws on the first promoted move.
 * That is why no existing test in this folder promotes a drag. Stub it to the
 * answer a real browser gives for a point over nothing, which is also the answer
 * every point gives in a document with no layout: null. The stub is the local
 * routing's whole story here, and that routing is deliberately not what this
 * file tests.
 */
beforeAll(() => {
  if (!document.elementFromPoint) {
    (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () =>
      null;
  }
});

afterAll(() => {
  delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
});

describe('useTabDragReorder — no cross-window callbacks (the shipped configuration)', () => {
  it('never derives a non-local phase, however far outside the pointer goes', () => {
    render(<Harness />);
    pressAndPromote();
    moveOutside();
    expect(screen.getByTestId('phase')).toHaveTextContent('local');
    // The gesture is still very much running — the point is that leaving the
    // window is simply not an event as far as this configuration is concerned.
    expect(screen.getByTestId('dragging')).toHaveTextContent('tab-1');
    expect(screen.getByTestId('ghost')).toHaveTextContent('yes');
  });

  it('does not consult isTabLocked when there is no cross-window half to refuse', () => {
    const isTabLocked = vi.fn(() => true);
    render(<Harness isTabLocked={isTabLocked} />);
    pressAndPromote();
    moveOutside();
    fireEvent.pointerUp(window, { pointerId: 1 });
    // The lock exists to gate exactly one branch. With that branch absent,
    // asking is pointless work and would make a caller believe the lock is doing
    // something. (It is also not asked while the pointer is inside the window,
    // where the answer could not change any outcome.)
    expect(isTabLocked).not.toHaveBeenCalled();
  });

  it('still commits an ordinary drag on pointerup', () => {
    // The reorder itself needs elementFromPoint, which jsdom cannot do, so this
    // asserts the shape of the ending: state cleared, no throw, no cross-window
    // callback invented out of nowhere.
    render(<Harness />);
    pressAndPromote();
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(screen.getByTestId('dragging')).toHaveTextContent('none');
    expect(screen.getByTestId('ghost')).toHaveTextContent('no');
    expect(screen.getByTestId('phase')).toHaveTextContent('local');
  });
});

describe('useTabDragReorder — cross-window phase', () => {
  it('reports detach when the pointer leaves the window, with the screen point', () => {
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} />);
    pressAndPromote();
    onCrossWindow.mockClear();
    moveOutside();

    expect(onCrossWindow).toHaveBeenCalledWith(
      { kind: 'detach' },
      { screenX: VIEWPORT_W + 50, screenY: 120 }
    );
    expect(screen.getByTestId('phase')).toHaveTextContent('detach');
  });

  it('re-reports detach on EVERY move outside — main must re-resolve a live point', () => {
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} />);
    pressAndPromote();
    onCrossWindow.mockClear();
    moveOutside();
    moveOutside();
    moveOutside();
    // Three moves, three resolutions. A phase deduped here would freeze the
    // target window's caret at wherever the pointer first crossed the edge.
    expect(onCrossWindow).toHaveBeenCalledTimes(3);
  });

  it('reports local ONCE on the way back in, not on every in-window move', () => {
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} />);
    pressAndPromote();
    moveOutside();
    onCrossWindow.mockClear();

    fireEvent.pointerMove(window, { pointerId: 1, clientX: 300, clientY: 20 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 310, clientY: 20 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 320, clientY: 20 });

    expect(onCrossWindow).toHaveBeenCalledTimes(1);
    expect(onCrossWindow).toHaveBeenCalledWith({ kind: 'local' }, expect.anything());
    expect(screen.getByTestId('phase')).toHaveTextContent('local');
  });

  it('does not promote a press that never moved: no phase, no callbacks', () => {
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} />);
    fireEvent.pointerDown(screen.getByTestId('tab'), {
      button: 0,
      pointerId: 1,
      clientX: 100,
      clientY: 20,
    });
    // Two pixels: a click, not a drag.
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 102, clientY: 20 });
    expect(onCrossWindow).not.toHaveBeenCalled();
  });
});

describe('useTabDragReorder — D1, a locked tab cannot leave the window', () => {
  it('refuses the cross-window half while the tab is locked', () => {
    const onCrossWindow = vi.fn();
    const onCrossWindowCommit = vi.fn();
    render(
      <Harness
        onCrossWindow={onCrossWindow}
        onCrossWindowCommit={onCrossWindowCommit}
        isTabLocked={(tabId) => tabId === 'tab-1'}
      />
    );
    pressAndPromote();
    moveOutside();
    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(onCrossWindow).not.toHaveBeenCalled();
    expect(onCrossWindowCommit).not.toHaveBeenCalled();
    expect(screen.getByTestId('phase')).toHaveTextContent('local');
  });

  it('locks by TAB, not globally — another tab is unaffected', () => {
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} isTabLocked={(tabId) => tabId === 'other'} />);
    pressAndPromote();
    moveOutside();
    expect(onCrossWindow).toHaveBeenCalledWith({ kind: 'detach' }, expect.anything());
  });

  it('is re-asked on every move, so a turn that ends mid-drag unlocks the tab', () => {
    let running = true;
    const onCrossWindow = vi.fn();
    render(<Harness onCrossWindow={onCrossWindow} isTabLocked={() => running} />);
    pressAndPromote();
    moveOutside();
    expect(onCrossWindow).not.toHaveBeenCalled();

    running = false; // the turn finished while the user was still dragging
    moveOutside();
    expect(onCrossWindow).toHaveBeenCalledWith({ kind: 'detach' }, expect.anything());
  });
});

describe('useTabDragReorder — commit and cancel', () => {
  it('commits the cross-window drop instead of the local one', () => {
    const onReorder = vi.fn();
    const onDropToGroup = vi.fn();
    const onCrossWindowCommit = vi.fn();
    render(
      <Harness
        onReorder={onReorder}
        onDropToGroup={onDropToGroup}
        onCrossWindowCommit={onCrossWindowCommit}
      />
    );
    pressAndPromote();
    moveOutside();
    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(onCrossWindowCommit).toHaveBeenCalledWith(
      { kind: 'detach' },
      { screenX: VIEWPORT_W + 50, screenY: 120 }
    );
    // Both halves committing would move the tab twice.
    expect(onReorder).not.toHaveBeenCalled();
    expect(onDropToGroup).not.toHaveBeenCalled();
  });

  it('Escape cancels: nothing commits and the gesture clears (D8)', () => {
    const onReorder = vi.fn();
    const onCrossWindowCommit = vi.fn();
    const onCrossWindow = vi.fn();
    render(
      <Harness
        onReorder={onReorder}
        onCrossWindow={onCrossWindow}
        onCrossWindowCommit={onCrossWindowCommit}
      />
    );
    pressAndPromote();
    moveOutside();
    onCrossWindow.mockClear();

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(onCrossWindowCommit).not.toHaveBeenCalled();
    expect(onReorder).not.toHaveBeenCalled();
    // The remote preview is cleared by saying the gesture is local again.
    expect(onCrossWindow).toHaveBeenCalledWith({ kind: 'local' }, expect.anything());
    expect(screen.getByTestId('dragging')).toHaveTextContent('none');
    expect(screen.getByTestId('ghost')).toHaveTextContent('no');
    expect(screen.getByTestId('phase')).toHaveTextContent('local');
  });

  it('a pointerup AFTER an Escape commits nothing — the gesture is already over', () => {
    const onCrossWindowCommit = vi.fn();
    render(<Harness onCrossWindowCommit={onCrossWindowCommit} />);
    pressAndPromote();
    moveOutside();
    fireEvent.keyDown(window, { key: 'Escape' });
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(onCrossWindowCommit).not.toHaveBeenCalled();
  });

  it('Escape does nothing when no drag has been promoted', () => {
    // Escape belongs to the rest of the app the other 99.9% of the time.
    const onReorder = vi.fn();
    render(<Harness onReorder={onReorder} />);
    fireEvent.keyDown(window, { key: 'Escape' });
    fireEvent.pointerDown(screen.getByTestId('tab'), {
      button: 0,
      pointerId: 1,
      clientX: 100,
      clientY: 20,
    });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.getByTestId('dragging')).toHaveTextContent('none');
  });

  it('ignores keys that are not Escape', () => {
    const onCrossWindowCommit = vi.fn();
    render(<Harness onCrossWindowCommit={onCrossWindowCommit} />);
    pressAndPromote();
    moveOutside();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(screen.getByTestId('dragging')).toHaveTextContent('tab-1');
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(onCrossWindowCommit).toHaveBeenCalledTimes(1);
  });

  it('a drag that returns inside before release commits locally, not cross-window', () => {
    const onCrossWindowCommit = vi.fn();
    render(<Harness onCrossWindowCommit={onCrossWindowCommit} />);
    pressAndPromote();
    moveOutside();
    fireEvent.pointerMove(window, {
      pointerId: 1,
      clientX: VIEWPORT_W / 2,
      clientY: VIEWPORT_H / 2,
    });
    fireEvent.pointerUp(window, { pointerId: 1 });
    // D6b: re-entering the source window is the ordinary local path, with no
    // special case and no latched detach.
    expect(onCrossWindowCommit).not.toHaveBeenCalled();
  });
});

describe('useTabDragReorder — pointer capture', () => {
  it('captures the pointer on the element that received the pointerdown, and releases it', () => {
    // jsdom does not implement capture, so the calls are stubbed onto the
    // prototype. What is asserted is the TARGET: capturing on the `.br-tab`
    // wrapper instead would retarget the compatibility click away from the label
    // button and silently break selecting a tab by clicking it.
    const setPointerCapture = vi.fn();
    const releasePointerCapture = vi.fn();
    const proto = window.HTMLElement.prototype as unknown as Record<string, unknown>;
    const originalSet = proto.setPointerCapture;
    const originalRelease = proto.releasePointerCapture;
    proto.setPointerCapture = setPointerCapture;
    proto.releasePointerCapture = releasePointerCapture;
    try {
      render(<Harness />);
      const tab = screen.getByTestId('tab');
      pressAndPromote();
      expect(setPointerCapture).toHaveBeenCalledWith(1);
      expect(setPointerCapture.mock.instances[0]).toBe(tab);

      fireEvent.pointerUp(window, { pointerId: 1 });
      expect(releasePointerCapture).toHaveBeenCalledWith(1);
      expect(releasePointerCapture.mock.instances[0]).toBe(tab);
    } finally {
      proto.setPointerCapture = originalSet;
      proto.releasePointerCapture = originalRelease;
    }
  });

  it('survives a capture that throws — the drag must not die in a pointerdown handler', () => {
    const proto = window.HTMLElement.prototype as unknown as Record<string, unknown>;
    const original = proto.setPointerCapture;
    proto.setPointerCapture = () => {
      throw new Error('NotFoundError');
    };
    try {
      render(<Harness />);
      pressAndPromote();
      expect(screen.getByTestId('dragging')).toHaveTextContent('tab-1');
      fireEvent.pointerUp(window, { pointerId: 1 });
      expect(screen.getByTestId('dragging')).toHaveTextContent('none');
    } finally {
      proto.setPointerCapture = original;
    }
  });
});
