import { useEffect, useRef, useState, PointerEvent } from 'react';
import { DropTarget, dropTargetAtPoint } from './dropZones';

const DRAG_PROMOTION_THRESHOLD_PX = 5;

export interface DragGhost {
  tabId: string;
  title: string;
  x: number;
  y: number;
}

interface TabDragReorderArgs {
  onReorder: (draggedTabId: string, targetTabId: string) => void;
  /**
   * Cross-group drop. Absent (as in ChatTabStrip's own fallback instance, and in
   * the strip's unit tests) => reorder-only, exactly the Stage-2 behaviour.
   */
  onDropToGroup?: (tabId: string, target: DropTarget) => void;
}

export interface TabDragReorder {
  draggedTabId: string | null;
  dragOverTabId: string | null;
  /** The live drop target, updated on POINTERMOVE — see the note below. */
  dropTarget: DropTarget | null;
  ghost: DragGhost | null;
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
 */
export function useTabDragReorder({ onReorder, onDropToGroup }: TabDragReorderArgs): TabDragReorder {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const [ghost, setGhost] = useState<DragGhost | null>(null);

  const gestureRef = useRef<{
    tabId: string;
    title: string;
    sourceGroupId?: string;
    startX: number;
    startY: number;
  } | null>(null);
  const draggedTabIdRef = useRef<string | null>(null);
  const dragOverTabIdRef = useRef<string | null>(null);
  const dropTargetRef = useRef<DropTarget | null>(null);
  const suppressTabClickRef = useRef(false);
  const suppressTabClickTimerRef = useRef<number | null>(null);

  const onReorderRef = useRef(onReorder);
  onReorderRef.current = onReorder;
  const onDropToGroupRef = useRef(onDropToGroup);
  onDropToGroupRef.current = onDropToGroup;

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
      setGhost({ tabId: gesture.tabId, title: gesture.title, x: event.clientX, y: event.clientY });

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

      if (!onDropToGroupRef.current) {
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

    const finish = () => {
      const sourceTabId = draggedTabIdRef.current;
      const targetTabId = dragOverTabIdRef.current;
      const target = dropTargetRef.current;

      if (sourceTabId && targetTabId) {
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
      setDraggedTabId(null);
      setDragOverTabId(null);
      setDropTarget(null);
      setGhost(null);
    };

    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', finish);
    window.addEventListener('pointercancel', finish);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', finish);
      window.removeEventListener('pointercancel', finish);
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
    gestureRef.current = {
      tabId,
      title,
      sourceGroupId,
      startX: event.clientX,
      startY: event.clientY,
    };
  };

  const guardClick = () => {
    if (!suppressTabClickRef.current) return false;
    suppressTabClickRef.current = false;
    window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
    suppressTabClickTimerRef.current = null;
    return true;
  };

  return { draggedTabId, dragOverTabId, dropTarget, ghost, beginDrag, guardClick };
}
