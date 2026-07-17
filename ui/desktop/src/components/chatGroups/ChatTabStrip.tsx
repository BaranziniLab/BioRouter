import { CSSProperties, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, MessageSquare, X } from '../icons/app-icons';
import { cn } from '../../utils';
import { getSessionTitlePadding } from '../Layout/TitlebarControls';
import { shouldShowTabOverflowMenu } from '../Layout/yieldLadder';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { ChatTab, ChatTabId, ChatGroupId } from './chatGroupsTypes';
import { useTabDragReorder } from './useTabDragReorder';
import { useChatTabDrag } from './ChatTabDragContext';

export interface ChatTabStripProps {
  tabs: ChatTab[];
  activeTabId: ChatTabId | null;
  /** The group this strip belongs to. Rides along with each drag gesture so the
   *  shared hook can tell a same-strip reorder from a cross-group move. */
  groupId?: ChatGroupId;
  /**
   * False when another group has focus. Only the FOCUSED group's active tab is
   * the painted pill; an inactive group's active tab keeps a neutral fill and no
   * pill shadow (spec: "Focus costs zero extra chrome"). Defaults to true so the
   * single-group case — and the strip's own tests — are unchanged.
   */
  groupActive?: boolean;
  /** Session ids with a live turn — from useRunningChats(). */
  runningSessionIds: readonly string[];
  onSelect: (tabId: ChatTabId) => void;
  onClose: (tabId: ChatTabId) => void;
  onReorder: (draggedTabId: ChatTabId, targetTabId: ChatTabId) => void;
  /**
   * True only for the FIRST leaf of the layout tree — computed by the shell with
   * firstLeaf(), a tree walk, never an array index. See chatGroupsTypes.firstLeaf.
   */
  reserveTitlebar: boolean;
  isCompactSidebarOverlayOpen: boolean;
  endSlot?: React.ReactNode;
}

export function ChatTabStrip({
  tabs,
  activeTabId,
  groupId,
  groupActive = true,
  runningSessionIds,
  onSelect,
  onClose,
  onReorder,
  reserveTitlebar,
  isCompactSidebarOverlayOpen,
  endSlot,
}: ChatTabStripProps) {
  // The shell provides ONE gesture for every strip, because a cross-group drag
  // must tint a group this strip does not render. Bare (as in this component's
  // own tests) the strip falls back to a private, reorder-only instance — which
  // is exactly the Stage-2 behaviour, unchanged.
  const sharedDrag = useChatTabDrag();
  const ownDrag = useTabDragReorder({ onReorder });
  const { draggedTabId, dragOverTabId, beginDrag, guardClick } = sharedDrag ?? ownDrag;
  const activeTabRef = useRef<HTMLButtonElement | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const [showOverflowMenu, setShowOverflowMenu] = useState(false);

  // Keep the focused tab in view as the strip scrolls past its shrink floor.
  useEffect(() => {
    // Feature-detected: scrollIntoView is absent in jsdom, and keeping the
    // focused tab visible must never be able to take the strip down.
    activeTabRef.current?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [activeTabId, tabs.length]);

  /**
   * Rung 3 of the yield ladder (D-32): shrink to the 88px floor, then scroll,
   * then collapse into a ▾ menu — never wrap.
   *
   * The first two steps are pure CSS (`.br-tabstrip`: flex-wrap: nowrap;
   * overflow-x: auto) and already shipped. This is the last one: once tabs are
   * scrolled out of sight, the ▾ is how you reach them without scrubbing.
   */
  const measureOverflow = useCallback(() => {
    const strip = stripRef.current;
    if (!strip) return;
    setShowOverflowMenu(
      shouldShowTabOverflowMenu({
        scrollWidth: strip.scrollWidth,
        clientWidth: strip.clientWidth,
        tabCount: tabs.length,
      })
    );
  }, [tabs.length]);

  // Two triggers, because they are two different events and neither implies the
  // other: the OBSERVER catches the strip's box changing (window resize, sidebar
  // collapse, splitter drag), and the layout effect catches the CONTENT changing
  // (a tab opened, closed or renamed) — which moves scrollWidth while leaving the
  // observed box identical, so the observer never fires for it.
  useLayoutEffect(() => {
    measureOverflow();
  }, [measureOverflow, tabs, activeTabId]);

  useEffect(() => {
    const strip = stripRef.current;
    if (!strip || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measureOverflow);
    observer.observe(strip);
    return () => observer.disconnect();
  }, [measureOverflow]);

  const handleOverflowSelect = useCallback(
    (tabId: ChatTabId) => {
      // Selecting scrolls it into view through the effect above — the menu is a
      // way to REACH a scrolled-out tab, so landing on it without showing it
      // would be half an answer.
      onSelect(tabId);
    },
    [onSelect]
  );

  // getSessionTitlePadding moved here from BaseChat: the strip is now its only
  // consumer. The reserve is what stops the tabs sliding under the macOS traffic
  // lights when the sidebar is collapsed — and when it fails, it fails SILENTLY.
  const paddingLeft = getSessionTitlePadding(isCompactSidebarOverlayOpen, reserveTitlebar);

  const handleKeyDown = (event: React.KeyboardEvent, index: number) => {
    // Roving tabindex: both tab surfaces have promised role="tablist" and never
    // delivered arrow-key navigation. This one does.
    if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
    event.preventDefault();
    const delta = event.key === 'ArrowRight' ? 1 : -1;
    const next = tabs[(index + delta + tabs.length) % tabs.length];
    if (next) onSelect(next.tabId);
  };

  return (
    // The wrap exists so the ▾ can sit OUTSIDE the strip's scroll box. Inside it
    // — even pinned with position: sticky — the button would occupy the strip's
    // own scrollWidth and keep alive the very overflow that summoned it: a latch
    // that never lets go. Outside, showing it only narrows clientWidth (an
    // overflowing strip overflows more) and hiding it only widens clientWidth (a
    // fitting strip fits better). Both directions are monotone, so the two states
    // cannot chase each other and the rung needs no hysteresis. See
    // shouldShowTabOverflowMenu.
    <div
      className="br-tabstrip-wrap flex h-full min-w-0 flex-1 items-center"
      style={{ WebkitAppRegion: 'drag' } as CSSProperties}
    >
      <div
        role="tablist"
        aria-label="Open chats"
        data-testid="chat-tab-strip"
        ref={stripRef}
        // The drag hit-tests THIS before it hit-tests the group's zones: a strip
        // sits entirely inside its group's `top` edge band, so routing it through
        // zoneFromRect would read every in-strip drag as "split upward" and
        // reorder would be unreachable. See useTabDragReorder's move handler.
        data-tab-strip-group={groupId}
        data-group-active={groupActive ? 'true' : 'false'}
        className="br-tabstrip br-tabstrip--inline h-full min-w-0 flex-1"
        // The strip lives INSIDE BaseChat's 52px WebkitAppRegion:'drag' header.
        // R1 was measured with real OS input through CDP: a pointer drag on a
        // no-drag child inside that drag region DOES reach the DOM and the window
        // does not move. So the strip may live here — but every tab must still
        // declare no-drag itself, or the OS eats the gesture before React sees it.
        style={{ paddingLeft, WebkitAppRegion: 'drag' } as CSSProperties}
      >
        {tabs.map((tab, index) => {
          const isActive = tab.tabId === activeTabId;
          const isRunning = runningSessionIds.includes(tab.sessionId);
          return (
            <div
              key={tab.tabId}
              data-tab-id={tab.tabId}
              data-active={isActive ? 'true' : undefined}
              data-dragging={draggedTabId === tab.tabId ? 'true' : undefined}
              data-dropbefore={dragOverTabId === tab.tabId ? 'true' : undefined}
              className={cn('br-tab group')}
              style={{ WebkitAppRegion: 'no-drag' } as CSSProperties}
            >
              <button
                ref={isActive ? activeTabRef : undefined}
                type="button"
                role="tab"
                aria-selected={isActive}
                tabIndex={isActive ? 0 : -1}
                title={tab.title}
                className="flex min-w-0 flex-1 items-center gap-[7px] bg-transparent text-left"
                onPointerDown={(event) => beginDrag(event, tab.tabId, tab.title, groupId)}
                onKeyDown={(event) => handleKeyDown(event, index)}
                onClick={() => {
                  // Swallow the synthetic click that ends a drag — otherwise
                  // dropping a tab also activates whatever you dropped it on.
                  if (guardClick()) return;
                  onSelect(tab.tabId);
                }}
              >
                <MessageSquare className="h-4 w-4 flex-none" />
                <span className="br-tab__label">{tab.title}</span>
              </button>

              {isRunning ? (
                // The pulse sits where the close control would be, and the control
                // returns on hover. One glyph, one meaning.
                <>
                  <span
                    className="br-tab__dot group-hover:hidden"
                    data-testid={`chat-tab-running-${tab.tabId}`}
                    aria-label="Running"
                    role="img"
                  />
                  <button
                    type="button"
                    aria-label={`Close ${tab.title}`}
                    data-testid={`chat-tab-close-${tab.tabId}`}
                    className="hidden flex-none rounded-full p-0.5 text-text-muted hover:bg-background-medium hover:text-text-default group-hover:block"
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={(event) => {
                      event.stopPropagation();
                      onClose(tab.tabId);
                    }}
                  >
                    <X className="h-4 w-4" />
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  aria-label={`Close ${tab.title}`}
                  data-testid={`chat-tab-close-${tab.tabId}`}
                  className="flex-none rounded-full p-0.5 text-text-muted opacity-0 transition-opacity hover:bg-background-medium hover:text-text-default focus-visible:opacity-100 group-hover:opacity-100"
                  onPointerDown={(event) => event.stopPropagation()}
                  onClick={(event) => {
                    event.stopPropagation();
                    onClose(tab.tabId);
                  }}
                >
                  <X className="h-4 w-4" />
                </button>
              )}
            </div>
          );
        })}
        {endSlot ? (
          <div className="flex-none" style={{ WebkitAppRegion: 'no-drag' } as CSSProperties}>
            {endSlot}
          </div>
        ) : null}
      </div>

      {showOverflowMenu && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Show all chats"
              data-testid="chat-tab-overflow-trigger"
              className="br-tabstrip__overflow"
              style={{ WebkitAppRegion: 'no-drag' } as CSSProperties}
            >
              <ChevronDown className="h-4 w-4" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="max-h-[60vh] w-56 overflow-y-auto">
            {tabs.map((tab) => (
              <DropdownMenuItem
                key={tab.tabId}
                data-testid={`chat-tab-overflow-item-${tab.tabId}`}
                onSelect={() => handleOverflowSelect(tab.tabId)}
                className={cn('gap-2', tab.tabId === activeTabId && 'font-medium')}
              >
                <MessageSquare className="h-4 w-4 flex-none" />
                <span className="min-w-0 flex-1 truncate">{tab.title}</span>
                {runningSessionIds.includes(tab.sessionId) ? (
                  // The same glyph the tab carries. One glyph, one meaning — a
                  // second vocabulary for "running" inside the menu would be a
                  // third answer to a question the strip already answers.
                  <span className="br-tab__dot flex-none" aria-label="Running" role="img" />
                ) : null}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

export default ChatTabStrip;
