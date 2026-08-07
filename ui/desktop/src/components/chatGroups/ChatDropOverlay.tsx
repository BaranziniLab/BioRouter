import { CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import { MessageSquare } from '../icons/app-icons';
import { DropZone } from './dropZones';
import { DragGhost } from './useTabDragReorder';

/** Where the tinted half sits for each zone. `center` covers the whole group. */
const ZONE_INSET: Record<DropZone, CSSProperties> = {
  center: { inset: 0 },
  left: { top: 0, bottom: 0, left: 0, right: '50%' },
  right: { top: 0, bottom: 0, left: '50%', right: 0 },
  top: { left: 0, right: 0, top: 0, bottom: '50%' },
  bottom: { left: 0, right: 0, top: '50%', bottom: 0 },
};

/**
 * The landing half, tinted WHILE THE TAB IS STILL IN THE AIR.
 *
 * POINTER-EVENTS: NONE IS LOAD-BEARING, NOT COSMETIC. The zone is resolved by
 * document.elementFromPoint (dropZones.ts), which returns the TOPMOST element at
 * the cursor. This overlay is absolutely positioned over the group it belongs
 * to, so the instant it appeared it would become that topmost element — the hit
 * test would keep resolving to the half that tinted first, and the zone would
 * latch there: you could never aim at the other half, and you could never leave
 * the group. The rule is in the CSS too (.br-dropzone); it is repeated here
 * because a later reader is far more likely to add `onClick` to this component
 * than to read the stylesheet.
 */
export function ChatDropOverlay({ zone }: { zone: DropZone }) {
  return (
    <div
      className="br-dropzone"
      data-testid="chat-drop-overlay"
      data-zone={zone}
      aria-hidden="true"
      style={ZONE_INSET[zone]}
    />
  );
}

/**
 * The tab, lifted under the cursor: fixed, 2deg, popover shadow (spec card ∇).
 *
 * Also pointer-events:none, and for a sharper reason than the overlay — the
 * ghost sits directly UNDER the cursor by construction, so if it took events it
 * would shadow every group equally and elementFromPoint would return the ghost
 * on every single move. The drop target would be null for the entire drag and
 * nothing would ever land.
 *
 * The ghost is a SEPARATE element; the source tab stays in flow at 35%
 * (.br-tab[data-dragging]). main.css's drag block explains why that matters:
 * .br-tab's divider is adjacent-sibling based, so pulling the real tab out of
 * the DOM would re-flow every divider in the strip mid-drag.
 *
 * `detached` marks the moment the drag left the window and the drop would make a
 * new one (tear-off design D7). Electron cannot paint outside a window's frame,
 * so the ghost cannot follow the cursor onto the desktop; instead it changes
 * what it IS — the tilt returns to 0 and it takes the dashed accent outline the
 * drop zones already use, because a flat, outlined rectangle detached from the
 * strip reads as a window and a tilted one riding the strip reads as a tab.
 *
 * THE ATTRIBUTE IS HERE AND THE CSS IS NOT. `.br-tab-ghost[data-detach='true']`
 * lands with the rest of the tear-off visuals (Phase 4); main.css is contested
 * by another campaign in flight. Until then this renders an attribute nothing
 * styles, which is inert rather than half-done: no consumer passes `detached`
 * yet either, and the attribute is absent (not `"false"`) when it is not set, so
 * the selector cannot half-match in the meantime.
 */
export function ChatTabGhost({ ghost, detached }: { ghost: DragGhost; detached?: boolean }) {
  // PORTALED TO document.body, and that is load-bearing, not tidiness. The ghost
  // is `position: fixed`, so its left/top are meant to be viewport coordinates —
  // which is exactly what useTabDragReorder computes (clientX/Y minus the grab
  // offset). But the shell renders inside `.route-container`, which sets
  // `contain: layout paint`; per CSS containment that element becomes the
  // containing block for fixed descendants, so the ghost's left/top would
  // resolve against `<main>` — offset from the viewport by the sidebar width
  // (~240px) — and the tab would float ~240px to the right of the cursor.
  // Portaling to <body> escapes any contained/transformed ancestor so the fixed
  // coordinates are true viewport coordinates and the tab rides under the cursor.
  if (typeof document === 'undefined') return null;
  return createPortal(
    <div
      className="br-tab br-tab-ghost"
      data-testid="chat-tab-ghost"
      data-detach={detached ? 'true' : undefined}
      aria-hidden="true"
      style={{ left: ghost.x, top: ghost.y }}
    >
      <MessageSquare className="h-4 w-4 flex-none" />
      <span className="br-tab__label">{ghost.title}</span>
    </div>,
    document.body
  );
}
