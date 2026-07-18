import { createContext, useContext } from 'react';
import { TabDragReorder } from './useTabDragReorder';

/**
 * ONE drag gesture, shared by every strip in the shell.
 *
 * A per-strip gesture cannot work once groups can split: the strip that receives
 * the pointerdown is in the SOURCE group, but the tint has to appear over the
 * TARGET group, which is a sibling component. Two components cannot read one
 * hook's state, so the gesture is hoisted to the shell and handed down.
 *
 * Null outside a provider — the ChatContext.tsx:74-77 pattern, used here so that
 * ChatTabStrip stays independently renderable (its unit tests mount it bare) and
 * falls back to its own reorder-only instance. It never throws.
 */
const ChatTabDragContext = createContext<TabDragReorder | null>(null);

export function useChatTabDrag(): TabDragReorder | null {
  return useContext(ChatTabDragContext);
}

export const ChatTabDragProvider = ChatTabDragContext.Provider;
