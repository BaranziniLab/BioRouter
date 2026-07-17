import { useEffect, useRef, useState, PointerEvent } from 'react';

const DRAG_PROMOTION_THRESHOLD_PX = 5;

interface TabDragReorderArgs {
  onReorder: (draggedTabId: string, targetTabId: string) => void;
}

interface TabDragReorder {
  draggedTabId: string | null;
  dragOverTabId: string | null;
  /** Attach to each tab's pointer-down target. Records origin only — no drag yet. */
  beginDrag: (event: PointerEvent<HTMLElement>, tabId: string) => void;
  /** Wrap the tab's click handler. Swallows the synthetic click a drag emits. */
  guardClick: () => boolean;
}

/**
 * Pointer-event tab reordering, mirroring ArtifactViewer's already-debugged
 * gesture (ArtifactViewer.tsx:380-430, :575-592) rather than rediscovering it:
 *
 *  1. pointerdown records the origin only. No drag yet — otherwise every click
 *     is a one-pixel drag.
 *  2. Promote past 5px. Mirrored into a ref AND state: the ref because the
 *     window listeners close over [] deps and must read fresh values; the state
 *     to drive the render.
 *  3. pointermove hit-tests with elementFromPoint + closest('[data-tab-id]'),
 *     NOT per-tab enter/leave — the hit test survives a scrolling, moving strip.
 *  4. pointerup reorders, then setTimeout(..., 0) clears suppressTabClickRef to
 *     swallow the synthetic click the browser fires after the gesture. Without
 *     this, finishing a drag also activates whatever tab you dropped on. That
 *     bug is not worth rediscovering.
 *
 * Cleanup runs on pointercancel too, or a cancelled gesture leaves the strip
 * stuck in a dragging state forever.
 *
 * NOT COVERED BY JSDOM: the 5px threshold's real geometry, elementFromPoint
 * (jsdom returns null — it computes no layout), and pointer capture. Those are
 * browser-verified, per the plan's §6 row 2.
 */
export function useTabDragReorder({ onReorder }: TabDragReorderArgs): TabDragReorder {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);

  const gestureRef = useRef<{ tabId: string; startX: number; startY: number } | null>(null);
  const draggedTabIdRef = useRef<string | null>(null);
  const dragOverTabIdRef = useRef<string | null>(null);
  const suppressTabClickRef = useRef(false);
  const suppressTabClickTimerRef = useRef<number | null>(null);
  const onReorderRef = useRef(onReorder);
  onReorderRef.current = onReorder;

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
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>('[data-tab-id]');
      const targetTabId = target?.dataset.tabId ?? null;
      const next = targetTabId === gesture.tabId ? null : targetTabId;
      dragOverTabIdRef.current = next;
      setDragOverTabId(next);
    };

    const finish = () => {
      const sourceTabId = draggedTabIdRef.current;
      const targetTabId = dragOverTabIdRef.current;
      if (sourceTabId && targetTabId) onReorderRef.current(sourceTabId, targetTabId);

      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
      suppressTabClickTimerRef.current = window.setTimeout(() => {
        suppressTabClickRef.current = false;
        suppressTabClickTimerRef.current = null;
      }, 0);

      gestureRef.current = null;
      draggedTabIdRef.current = null;
      dragOverTabIdRef.current = null;
      setDraggedTabId(null);
      setDragOverTabId(null);
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

  const beginDrag = (event: PointerEvent<HTMLElement>, tabId: string) => {
    if (event.button !== 0) return;
    gestureRef.current = { tabId, startX: event.clientX, startY: event.clientY };
  };

  const guardClick = () => {
    if (!suppressTabClickRef.current) return false;
    suppressTabClickRef.current = false;
    window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
    suppressTabClickTimerRef.current = null;
    return true;
  };

  return { draggedTabId, dragOverTabId, beginDrag, guardClick };
}
