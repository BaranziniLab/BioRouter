import { CSSProperties } from 'react';
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
 */
export function ChatTabGhost({ ghost }: { ghost: DragGhost }) {
  return (
    <div
      className="br-tab br-tab-ghost"
      data-testid="chat-tab-ghost"
      aria-hidden="true"
      style={{ left: ghost.x, top: ghost.y }}
    >
      <MessageSquare className="h-4 w-4 flex-none" />
      <span className="br-tab__label">{ghost.title}</span>
    </div>
  );
}
