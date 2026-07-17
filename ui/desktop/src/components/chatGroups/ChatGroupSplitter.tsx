import { PointerEvent as ReactPointerEvent, useCallback, useEffect, useRef } from 'react';

/**
 * The smallest a pane may be squeezed to, as a fraction of the branch.
 *
 * Without a floor a splitter dragged to the edge produces a 0-width pane that
 * still mounts a whole BaseChat: invisible, un-grabbable (its own splitter is
 * now under its neighbour's), and impossible to recover without closing its
 * tabs. 12% of a branch is narrow but always re-grabbable.
 */
const MIN_PANE_FRACTION = 0.12;

interface ChatGroupSplitterProps {
  dir: 'row' | 'col';
  /** Index of the child BEFORE this splitter. */
  index: number;
  sizes: readonly number[];
  containerRef: React.RefObject<HTMLDivElement | null>;
  onResize: (sizes: number[]) => void;
}

/**
 * The grab handle between two panes.
 *
 * Only the two panes ADJACENT to the handle resize; the rest of the branch is
 * untouched. Dragging the second of three splitters must not shove pane 1
 * around — the handle you grabbed is the boundary you are moving, and nothing
 * else should follow it.
 */
export function ChatGroupSplitter({
  dir,
  index,
  sizes,
  containerRef,
  onResize,
}: ChatGroupSplitterProps) {
  const dragRef = useRef<{ start: number; total: number; a: number; b: number } | null>(null);
  const onResizeRef = useRef(onResize);
  onResizeRef.current = onResize;
  const sizesRef = useRef(sizes);
  sizesRef.current = sizes;

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      const container = containerRef.current;
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const total = dir === 'row' ? rect.width : rect.height;
      if (total <= 0) return;
      event.preventDefault();
      dragRef.current = {
        start: dir === 'row' ? event.clientX : event.clientY,
        total,
        a: sizesRef.current[index],
        b: sizesRef.current[index + 1],
      };
    },
    [containerRef, dir, index]
  );

  useEffect(() => {
    const move = (event: globalThis.PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      event.preventDefault();
      const delta = (dir === 'row' ? event.clientX : event.clientY) - drag.start;
      const shift = delta / drag.total;
      // The pair's combined share is invariant: whatever one pane gives up, its
      // neighbour takes. This is what keeps the other panes still.
      const pair = drag.a + drag.b;
      const min = MIN_PANE_FRACTION * pair;
      const nextA = Math.min(Math.max(drag.a + shift, min), pair - min);
      const next = [...sizesRef.current];
      next[index] = nextA;
      next[index + 1] = pair - nextA;
      onResizeRef.current(next);
    };
    const finish = () => {
      dragRef.current = null;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', finish);
    window.addEventListener('pointercancel', finish);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', finish);
      window.removeEventListener('pointercancel', finish);
    };
  }, [dir, index]);

  return (
    <div
      role="separator"
      aria-orientation={dir === 'row' ? 'vertical' : 'horizontal'}
      aria-label="Resize chat groups"
      data-testid={`chat-group-splitter-${dir}-${index}`}
      className={dir === 'row' ? 'br-group-splitter br-group-splitter--row' : 'br-group-splitter'}
      onPointerDown={handlePointerDown}
    />
  );
}
