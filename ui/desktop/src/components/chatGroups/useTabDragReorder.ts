import { useEffect, useRef, useState, PointerEvent } from 'react';
import { DropTarget, dropTargetAtPoint } from './dropZones';
import { CrossWindowPhase, ScreenPoint, isOutsideViewport } from './tabTearOff';

const DRAG_PROMOTION_THRESHOLD_PX = 5;

/**
 * `local`, the phase every drag starts and (unless it leaves the window) ends
 * in. A shared constant so an identity comparison is a valid "nothing changed"
 * test for the common case.
 */
const LOCAL_PHASE: CrossWindowPhase = { kind: 'local' };

/**
 * THE ONE PLACE THAT ASSUMES ANYTHING ABOUT OS POINTER CAPTURE.
 *
 * Explicit capture is what the Pointer Events spec guarantees will keep
 * delivering `pointermove`/`pointerup` to the capturing element regardless of
 * where the cursor is hit-testing (design D2). The gesture worked before it
 * because implicit capture covers the in-window case; it is made explicit here
 * because a drag that leaves the window is precisely the case implicit capture
 * was never asked about.
 *
 * That capture survives the cursor crossing ANOTHER WINDOW OF THE SAME APP is
 * asserted from the spec, not measured — design §8 Phase 0 exists to measure it
 * with real OS input, which an agent shell cannot generate and jsdom does not
 * implement at all. Both calls are therefore confined to this pair of helpers:
 * if Phase 0 comes back with a different answer (say, capture must be taken on
 * `document.body`, or dropped in favour of a main-process pointer hook), one
 * function changes and the gesture above it does not.
 *
 * Failures are swallowed on purpose. `setPointerCapture` throws
 * `NotFoundError` for a pointer that is no longer active, and jsdom does not
 * implement it at all; in both cases the drag must carry on exactly as it did
 * before capture existed rather than dying in a pointerdown handler.
 */
function capturePointer(el: Element, pointerId: number): Element | null {
  try {
    (el as HTMLElement).setPointerCapture?.(pointerId);
    return el;
  } catch {
    return null;
  }
}

function releasePointer(el: Element | null, pointerId: number): void {
  if (!el) return;
  try {
    (el as HTMLElement).releasePointerCapture?.(pointerId);
  } catch {
    // Already released, element gone, or unimplemented. Nothing to undo.
  }
}

export interface DragGhost {
  tabId: string;
  title: string;
  x: number;
  y: number;
  /**
   * Where inside the tab the pointer grabbed it. `x`/`y` above are already
   * `client − grabOffset`, so this is redundant for POSITIONING — it is carried
   * because a tear-off needs it in screen space, to place the new window so the
   * tab lands under the cursor that is still holding it (design §5). Deriving it
   * back out of `x` would need the client point, which is not published.
   */
  grabOffsetX: number;
  grabOffsetY: number;
}

interface TabDragReorderArgs {
  onReorder: (draggedTabId: string, targetTabId: string) => void;
  /**
   * Cross-group drop. Absent (as in ChatTabStrip's own fallback instance, and in
   * the strip's unit tests) => reorder-only, exactly the Stage-2 behaviour.
   */
  onDropToGroup?: (tabId: string, target: DropTarget) => void;
  /**
   * The pointer left this window's content rect during a promoted drag, or came
   * back. Called with `{kind:'detach'}` on EVERY move while outside — the main
   * process must re-resolve the live screen point against its window registry to
   * decide whether this is really a merge (tabTearOff.ts, design D3) — and with
   * `{kind:'local'}` exactly ONCE when the pointer returns, since that phase
   * carries no point-dependent information and re-sending it per move would put
   * an IPC message on the wire at pointer rate for the overwhelmingly common
   * drag that never leaves the window at all.
   *
   * ABSENT (today, and in every existing test) => the pointer position outside
   * the window is not consulted at all and the gesture behaves byte for byte as
   * it always has.
   */
  onCrossWindow?: (phase: CrossWindowPhase, point: ScreenPoint) => void;
  /**
   * Release, outside the window. Called INSTEAD of `onReorder`/`onDropToGroup` —
   * the tab is leaving, so there is nothing local to commit. The phase is
   * whatever the last move derived, which is `detach` from the renderer's side;
   * the owner resolves it against main's registry, which is also the party that
   * knows whether tearing off would empty this window (design D5, a no-op).
   */
  onCrossWindowCommit?: (phase: CrossWindowPhase, point: ScreenPoint) => void;
}

/**
 * THERE IS NO RUNNING-TAB LOCK HERE, AND ADDING ONE WOULD NOW BE A REGRESSION.
 *
 * Phase 2 shipped an `isTabLocked?(tabId)` injection point for design D1 — a tab
 * whose session had a turn in flight could not leave the window, because
 * `/reply` had exactly one subscriber and moving it either froze the transcript
 * or killed the turn.
 *
 * D1 is superseded (design §3.1). `crates/biorouter-server/src/turn_stream.rs`
 * made a turn's frame log a session-scoped, replayable, many-observer object: a
 * departing subscriber no longer cancels anything, and `ChatStreamController`'s
 * `attachToTurn` rejoins a running turn from the sequence a window has already
 * painted — automatically, off `/agent/resume`, on every session load. A torn-off
 * tab therefore keeps streaming in its new window with no handover protocol at
 * all.
 *
 * The argument was deleted rather than left unused, because a dead hook parameter
 * named for a lock is how the lock comes back.
 */

export interface TabDragReorder {
  draggedTabId: string | null;
  dragOverTabId: string | null;
  /** The live drop target, updated on POINTERMOVE — see the note below. */
  dropTarget: DropTarget | null;
  ghost: DragGhost | null;
  /**
   * The cross-window phase, for rendering: `detach` is what puts the ghost into
   * its "this will become a window" state (design D7). Stays `local` for the
   * whole gesture unless a cross-window callback is wired, so no consumer that
   * does not ask for the feature can be affected by it.
   */
  crossWindow: CrossWindowPhase;
  /**
   * Attach to each tab's pointer-down target. Records origin only — no drag yet.
   * `sourceGroupId` rides along with the GESTURE rather than being a hook
   * argument because there is ONE hook instance for the whole shell (the overlay
   * has to render in the TARGET group, which is a different component from the
   * source strip), so the hook cannot know from its own props which group a drag
   * started in — only the strip that received the pointerdown knows that.
   */
  beginDrag: (
    event: PointerEvent<HTMLElement>,
    tabId: string,
    title: string,
    sourceGroupId?: string
  ) => void;
  /** Wrap the tab's click handler. Swallows the synthetic click a drag emits. */
  guardClick: () => boolean;
}

/**
 * Pointer-event tab dragging: reorder within a strip, move or split across
 * groups. Mirrors ArtifactViewer's already-debugged gesture
 * (ArtifactViewer.tsx:380-430, :575-592) rather than rediscovering it:
 *
 *  1. pointerdown records the origin only. No drag yet — otherwise every click
 *     is a one-pixel drag.
 *  2. Promote past 5px. Mirrored into a ref AND state: the ref because the
 *     window listeners close over [] deps and must read fresh values; the state
 *     to drive the render.
 *  3. pointermove hit-tests with elementFromPoint + closest, NOT per-tab
 *     enter/leave — the hit test survives a scrolling, moving strip.
 *  4. pointerup reorders/moves, then setTimeout(..., 0) clears
 *     suppressTabClickRef to swallow the synthetic click the browser fires after
 *     the gesture. Without this, finishing a drag also activates whatever tab you
 *     dropped on. That bug is not worth rediscovering.
 *
 * Cleanup runs on pointercancel too, or a cancelled gesture leaves the strip
 * stuck in a dragging state forever.
 *
 * THE DROP TARGET IS COMPUTED ON POINTERMOVE, NOT ON POINTERUP. That is the
 * requirement, not an implementation detail: spec card ∇ says the landing half
 * tints "while the tab is still in the air… you aim, then commit — never commit,
 * then discover." Resolving the zone on pointerup would make the tint a report
 * of what already happened. Everything the overlay needs must therefore be state
 * that moves with the cursor.
 *
 * NOT COVERED BY JSDOM: the 5px threshold's real geometry, elementFromPoint
 * (jsdom computes no layout and returns null), getBoundingClientRect (all zeroes
 * in jsdom, so every zone would read `center`), and pointer capture. Those are
 * browser-verified, per the plan's §6 row 2.
 *
 * The cross-window half (tear-off and merge) is bolted on the same way the
 * cross-group half was: OPTIONAL callbacks, and with none of them supplied the
 * gesture is bit-for-bit the one that shipped. See TabDragReorderArgs.
 */
export function useTabDragReorder({
  onReorder,
  onDropToGroup,
  onCrossWindow,
  onCrossWindowCommit,
}: TabDragReorderArgs): TabDragReorder {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const [ghost, setGhost] = useState<DragGhost | null>(null);
  const [crossWindow, setCrossWindow] = useState<CrossWindowPhase>(LOCAL_PHASE);

  const gestureRef = useRef<{
    tabId: string;
    title: string;
    sourceGroupId?: string;
    startX: number;
    startY: number;
    // Where inside the tab the cursor grabbed it, so the ghost rides under the
    // same point rather than hanging off the cursor's lower-right.
    grabOffsetX: number;
    grabOffsetY: number;
    // The element holding the OS pointer capture, and the pointer it holds. Kept
    // so `finish` can release exactly what `beginDrag` took — see capturePointer.
    captureEl: Element | null;
    pointerId: number;
  } | null>(null);
  const draggedTabIdRef = useRef<string | null>(null);
  const dragOverTabIdRef = useRef<string | null>(null);
  const dropTargetRef = useRef<DropTarget | null>(null);
  const crossWindowRef = useRef<CrossWindowPhase>(LOCAL_PHASE);
  const screenPointRef = useRef<ScreenPoint>({ screenX: 0, screenY: 0 });
  const suppressTabClickRef = useRef(false);
  const suppressTabClickTimerRef = useRef<number | null>(null);

  const onReorderRef = useRef(onReorder);
  onReorderRef.current = onReorder;
  const onDropToGroupRef = useRef(onDropToGroup);
  onDropToGroupRef.current = onDropToGroup;
  const onCrossWindowRef = useRef(onCrossWindow);
  onCrossWindowRef.current = onCrossWindow;
  const onCrossWindowCommitRef = useRef(onCrossWindowCommit);
  onCrossWindowCommitRef.current = onCrossWindowCommit;

  useEffect(() => {
    const move = (event: globalThis.PointerEvent) => {
      const gesture = gestureRef.current;
      if (!gesture) return;

      if (!draggedTabIdRef.current) {
        const distance = Math.hypot(event.clientX - gesture.startX, event.clientY - gesture.startY);
        if (distance < DRAG_PROMOTION_THRESHOLD_PX) return;
        draggedTabIdRef.current = gesture.tabId;
        suppressTabClickRef.current = true;
        setDraggedTabId(gesture.tabId);
      }

      event.preventDefault();
      // Place the ghost's top-left so the cursor lands on the exact spot inside
      // the tab where the drag began — the tab travels UNDER the cursor, not
      // trailing to its lower-right.
      setGhost({
        tabId: gesture.tabId,
        title: gesture.title,
        x: event.clientX - gesture.grabOffsetX,
        y: event.clientY - gesture.grabOffsetY,
        grabOffsetX: gesture.grabOffsetX,
        grabOffsetY: gesture.grabOffsetY,
      });

      // ===================================================================
      // IS THE POINTER STILL IN THIS WINDOW? (design D2)
      //
      // Gated on a cross-window callback being supplied. With none —
      // ChatTabStrip's bare fallback instance and every strip-level test —
      // `outside` is false forever and not one line below this block behaves
      // differently than it did before tear-off existed.
      //
      // NO SECOND CONDITION. This used to also ask whether the tab was locked by
      // a live turn (D1); see the note above TabDragReorderArgs for why that
      // question no longer exists.
      // ===================================================================
      const crossWindowEnabled = !!(onCrossWindowRef.current || onCrossWindowCommitRef.current);
      const outside =
        crossWindowEnabled &&
        isOutsideViewport(
          { x: event.clientX, y: event.clientY },
          { width: window.innerWidth, height: window.innerHeight }
        );

      screenPointRef.current = { screenX: event.screenX, screenY: event.screenY };
      const nextPhase: CrossWindowPhase = outside ? { kind: 'detach' } : LOCAL_PHASE;
      const phaseChanged = nextPhase.kind !== crossWindowRef.current.kind;
      crossWindowRef.current = nextPhase;
      if (phaseChanged) setCrossWindow(nextPhase);
      // Every move while outside (main must re-resolve the live point against its
      // window registry), but only the transition back in — see onCrossWindow.
      if (outside || phaseChanged) {
        onCrossWindowRef.current?.(nextPhase, screenPointRef.current);
      }

      // ===================================================================
      // THE STRIP IS ALWAYS INSIDE ITS GROUP'S `top` EDGE BAND.
      //
      // A strip is 52px tall at the top of a group several hundred px tall, so
      // every tab in it sits well within the top 25% — zoneFromRect over a tab
      // therefore answers `top`, never `center`. Resolving the strip through the
      // group's zones would mean dragging a tab one place to the left inside its
      // OWN strip resolved as "split this group upward": reorder, the commonest
      // gesture of the three, would be unreachable.
      //
      // So the strip is hit-tested FIRST and on its own terms, and the group
      // zones only apply to a drop on the group's BODY:
      //   - own strip     => reorder
      //   - other strip   => move into that group (what dropping on a tab bar
      //                      means everywhere else: insert here)
      //   - group body    => zoneFromRect: centre moves, an edge splits
      // ===================================================================
      const element = document.elementFromPoint(event.clientX, event.clientY);
      const stripGroupId =
        element?.closest<HTMLElement>('[data-tab-strip-group]')?.dataset.tabStripGroup ?? null;
      const targetTabId = element?.closest<HTMLElement>('[data-tab-id]')?.dataset.tabId ?? null;
      const overOtherTab = targetTabId !== null && targetTabId !== gesture.tabId;

      let nextOverTab: string | null = null;
      let nextTarget: DropTarget | null = null;

      if (outside) {
        // Nothing local to aim at. elementFromPoint would answer null out here
        // anyway; saying it on purpose means a detach can never also carry a
        // stale local target into `finish` and commit both halves of the gesture.
        nextOverTab = null;
        nextTarget = null;
      } else if (!onDropToGroupRef.current) {
        // Reorder-only: no cross-group handler, so this is a bare strip. Exactly
        // the Stage-2 behaviour, unchanged.
        nextOverTab = overOtherTab ? targetTabId : null;
      } else if (stripGroupId !== null && stripGroupId === gesture.sourceGroupId) {
        nextOverTab = overOtherTab ? targetTabId : null;
      } else if (stripGroupId !== null) {
        nextTarget = { groupId: stripGroupId, zone: 'center' };
      } else {
        nextTarget = dropTargetAtPoint(event.clientX, event.clientY);
      }

      dragOverTabIdRef.current = nextOverTab;
      dropTargetRef.current = nextTarget;
      setDragOverTabId(nextOverTab);
      setDropTarget(nextTarget);
    };

    const finish = (cancelled = false) => {
      const sourceTabId = draggedTabIdRef.current;
      const targetTabId = dragOverTabIdRef.current;
      const target = dropTargetRef.current;
      const phase = crossWindowRef.current;
      const gesture = gestureRef.current;

      if (gesture) releasePointer(gesture.captureEl, gesture.pointerId);

      if (cancelled) {
        // D8: no reorder, no move, no tear-off, no merge. The REMOTE preview is
        // cleared by reporting `local` — that phase means "no cross-window
        // preview" by definition, so there is no separate cancel message to
        // invent and no way for the owner to be left holding a stale caret.
        if (phase.kind !== 'local') {
          onCrossWindowRef.current?.(LOCAL_PHASE, screenPointRef.current);
        }
      } else if (sourceTabId && phase.kind !== 'local' && onCrossWindowCommitRef.current) {
        // The tab is leaving the window, so the local commits below are not just
        // unnecessary but wrong — they would move it twice.
        onCrossWindowCommitRef.current(phase, screenPointRef.current);
      } else if (sourceTabId && targetTabId) {
        onReorderRef.current(sourceTabId, targetTabId);
      } else if (sourceTabId && target && onDropToGroupRef.current) {
        onDropToGroupRef.current(sourceTabId, target);
      }

      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
      suppressTabClickTimerRef.current = window.setTimeout(() => {
        suppressTabClickRef.current = false;
        suppressTabClickTimerRef.current = null;
      }, 0);

      gestureRef.current = null;
      draggedTabIdRef.current = null;
      dragOverTabIdRef.current = null;
      dropTargetRef.current = null;
      crossWindowRef.current = LOCAL_PHASE;
      setDraggedTabId(null);
      setDragOverTabId(null);
      setDropTarget(null);
      setGhost(null);
      // LOCAL_PHASE is a module constant, so React bails out of this update by
      // identity when the phase never left `local` — which is every drag in the
      // shipped configuration. No extra render is added to today's gesture.
      setCrossWindow(LOCAL_PHASE);
    };

    // pointerup/pointercancel COMMIT, both of them, exactly as they always have.
    // Only Escape cancels (D8) — pointercancel's commit is long-standing shipped
    // behaviour and changing it is not this feature's business.
    const finishFromPointer = () => finish();

    const cancelOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      // Only a PROMOTED drag. A press that has not passed the 5px threshold is
      // not a gesture the user can see, so cancelling it would swallow an Escape
      // the rest of the app is entitled to (closing a menu, blurring a field).
      if (!draggedTabIdRef.current) return;
      finish(true);
    };

    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', finishFromPointer);
    window.addEventListener('pointercancel', finishFromPointer);
    window.addEventListener('keydown', cancelOnEscape);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', finishFromPointer);
      window.removeEventListener('pointercancel', finishFromPointer);
      window.removeEventListener('keydown', cancelOnEscape);
      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
    };
  }, []);

  const beginDrag = (
    event: PointerEvent<HTMLElement>,
    tabId: string,
    title: string,
    sourceGroupId?: string
  ) => {
    if (event.button !== 0) return;
    // Measure the grab point against the whole tab (the ghost is styled as a
    // `.br-tab`), falling back to the pointer target if it is somehow detached.
    const tabEl =
      (event.currentTarget.closest('.br-tab') as HTMLElement | null) ?? event.currentTarget;
    const rect = tabEl.getBoundingClientRect();
    gestureRef.current = {
      tabId,
      title,
      sourceGroupId,
      startX: event.clientX,
      startY: event.clientY,
      grabOffsetX: event.clientX - rect.left,
      grabOffsetY: event.clientY - rect.top,
      // CAPTURE ON THE ELEMENT THAT RECEIVED THE POINTERDOWN, NOT ON `.br-tab`.
      // The wrapper looks like the more natural owner and is the wrong choice:
      // pointer capture retargets the compatibility mouse events too, so
      // capturing on the wrapper would fire `click` on the wrapper — a plain
      // click on a tab would then never reach the label button's onClick that
      // selects it, and selecting a tab by clicking it would quietly stop
      // working. jsdom does not implement capture, so no test here would catch
      // that; only a real browser would. Capturing on the pointerdown target
      // leaves the click exactly where it already was.
      captureEl: capturePointer(event.currentTarget, event.pointerId),
      pointerId: event.pointerId,
    };
  };

  const guardClick = () => {
    if (!suppressTabClickRef.current) return false;
    suppressTabClickRef.current = false;
    window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
    suppressTabClickTimerRef.current = null;
    return true;
  };

  return { draggedTabId, dragOverTabId, dropTarget, ghost, crossWindow, beginDrag, guardClick };
}
