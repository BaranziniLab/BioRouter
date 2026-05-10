import { useCallback, useRef } from 'react';
import React from 'react';

export interface DragOptions {
  onMove?: (delta: { dx: number; dy: number }, e: PointerEvent) => void;
  onEnd?: (delta: { dx: number; dy: number }, e: PointerEvent) => void;
  onCancel?: () => void;
}

/**
 * Returns a pointerdown handler that captures a drag and reports deltas.
 * Cleans up on pointerup or pointercancel.
 */
export function usePointerDrag(opts: DragOptions): (e: React.PointerEvent) => void {
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  return useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    startRef.current = start;

    const handleMove = (ev: PointerEvent) => {
      const s = startRef.current;
      if (!s) return;
      optsRef.current.onMove?.({ dx: ev.clientX - s.x, dy: ev.clientY - s.y }, ev);
    };
    const handleEnd = (ev: PointerEvent) => {
      const s = startRef.current;
      startRef.current = null;
      window.removeEventListener('pointermove', handleMove);
      window.removeEventListener('pointerup', handleEnd);
      window.removeEventListener('pointercancel', handleCancel);
      if (s) optsRef.current.onEnd?.({ dx: ev.clientX - s.x, dy: ev.clientY - s.y }, ev);
    };
    const handleCancel = () => {
      startRef.current = null;
      window.removeEventListener('pointermove', handleMove);
      window.removeEventListener('pointerup', handleEnd);
      window.removeEventListener('pointercancel', handleCancel);
      optsRef.current.onCancel?.();
    };

    window.addEventListener('pointermove', handleMove);
    window.addEventListener('pointerup', handleEnd);
    window.addEventListener('pointercancel', handleCancel);
  }, []);
}
