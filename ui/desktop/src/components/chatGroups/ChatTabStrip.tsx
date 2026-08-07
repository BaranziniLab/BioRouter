import { CSSProperties, useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { ChevronDown, MessageSquare, X } from '../icons/app-icons';
import { cn } from '../../utils';
import { getSessionTitlePadding } from '../Layout/TitlebarControls';
import { useTabStripOverflow } from '../Layout/useTabStripOverflow';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { ChatTab, ChatTabId, ChatGroupId } from './chatGroupsTypes';
import { useTabDragReorder } from './useTabDragReorder';
import { useChatTabDrag } from './ChatTabDragContext';
import type { TabAnnotation } from './workspaceCommandPlanner';

function prefersReducedMotion(): boolean {
  return (
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

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
  /**
   * BR-71 Task 26's tab annotations, keyed by session id.
   *
   * A PROP, not a `useChatGroups()` call, for the same reason `runningSessionIds`
   * is: this component is pure-props and three test files render it bare, outside
   * any provider. OPTIONAL with a `{}` default so those files keep compiling and
   * passing with no edit — the strip's whole contract is that it can be rendered
   * with nothing but its own props. The shell's own suites additionally mock
   * `useChatGroups` with hand-written stubs that have no `tabAnnotations`, so the
   * default is what keeps `groups.tabAnnotations` being `undefined` a no-op there.
   */
  tabAnnotations?: Record<string, TabAnnotation>;
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
  /**
   * The insertion caret for a CROSS-WINDOW merge — a tab being dragged into this
   * strip from another window.
   *
   * It needs its own prop because `dragOverTabId` cannot supply it. While the
   * mouse button is held, the OS delivers every pointer event to the window the
   * drag STARTED in; this window receives nothing at all (measured with real OS
   * input, design §8 Phase 0), so its own drag state stays empty for the whole
   * gesture. The caret is driven by IPC from the main process instead.
   *
   * The rendering is deliberately identical — the same `[data-dropbefore]`
   * hairline the local reorder draws. Merging into a strip IS inserting into a
   * strip; a second visual for it would be a second answer to a question the
   * strip already answers.
   */
  remoteDropBeforeTabId?: ChatTabId | null;
}

export function ChatTabStrip({
  tabs,
  activeTabId,
  groupId,
  groupActive = true,
  runningSessionIds,
  tabAnnotations = {},
  onSelect,
  onClose,
  onReorder,
  reserveTitlebar,
  isCompactSidebarOverlayOpen,
  endSlot,
  remoteDropBeforeTabId = null,
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

  // #37 — FLIP: tabs SLIDE to their new slots with the spring easing instead
  // of teleporting. Reorder is a pure array move in the reducer, so React
  // re-renders the strip with the tabs already in their final DOM positions;
  // this layout effect measures where each tab WAS (last snapshot), applies
  // the inverted translate, and lets a transition play it back to identity.
  // It keys on the rendered ORDER, so one pass covers drag-reorder, the shift
  // after a close, and a cross-group move — with no animation library.
  //
  // offsetLeft, not getBoundingClientRect: it is layout-relative, so a scrolled
  // strip or a moved window cannot poison the deltas between snapshots. In
  // jsdom every offset is 0, so the pass is a structural no-op there — the
  // spring itself is browser-verified (the repo's Prism/Tailwind lesson).
  const tabOffsetsRef = useRef<Map<string, number>>(new Map());
  const orderKey = tabs.map((t) => t.tabId).join('␟');

  useLayoutEffect(() => {
    const strip = stripRef.current;
    if (!strip) return;
    const previous = tabOffsetsRef.current;
    const next = new Map<string, number>();
    const els = Array.from(strip.querySelectorAll<HTMLElement>('[data-tab-id]'));
    for (const el of els) {
      if (el.dataset.tabId) next.set(el.dataset.tabId, el.offsetLeft);
    }
    tabOffsetsRef.current = next;

    if (prefersReducedMotion()) return;

    // Everything this pass schedules is retained so the cleanup can undo it:
    // an interrupted animation (unmount, or another reorder mid-slide) must
    // not leave a stale rAF callback scheduled or an inline transition
    // override shadowing the stylesheet's hover colours forever.
    const rafIds: number[] = [];
    const restores: Array<() => void> = [];

    for (const el of els) {
      const tabId = el.dataset.tabId;
      if (!tabId) continue;
      const before = previous.get(tabId);
      // A tab with no previous slot is newly opened — it appears in place
      // rather than sliding in from nowhere.
      if (before === undefined) continue;
      const delta = before - (next.get(tabId) ?? before);
      if (delta === 0) continue;

      // Invert without transitioning, then release to identity on the next
      // frame with the spring. The offset rides the individual `translate`
      // property, NOT the `transform` list: the br-tab-select settle animates
      // the individual `scale`, and per CSS Transforms 2 the CTM composes
      // translate → rotate → scale → transform — so a translateX() in
      // `transform` sits INSIDE the scale and gets shrunk with it (the active
      // successor would start off-slot while settling from 0.97), while the
      // individual `translate` composes OUTSIDE it and the slide stays exact
      // (Codex B6 re-review finding 4). Never left/top, and never a reparent —
      // the divider (::before) and drop hairline (::after) contracts stay
      // untouched.
      el.style.transition = 'none';
      el.style.translate = `${delta}px`;

      // Named + idempotent: reached from transitionend, transitioncancel AND
      // the effect cleanup, whichever comes first.
      const restore = () => {
        // Give the stylesheet back its own transition (hover colours).
        el.style.removeProperty('transition');
        el.style.removeProperty('translate');
        el.removeEventListener('transitionend', onDone);
        el.removeEventListener('transitioncancel', onDone);
      };
      const onDone = (event: Event) => {
        // Only OUR translate transition on THIS element: transitionend
        // bubbles, so a child's opacity fade (the close control) must not cut
        // the slide short. propertyName is best-effort — jsdom's synthetic
        // events omit it.
        if (event.target !== el) return;
        const propertyName = (event as TransitionEvent).propertyName;
        if (propertyName && propertyName !== 'translate') return;
        restore();
      };
      restores.push(restore);

      rafIds.push(
        window.requestAnimationFrame(() => {
          if (!el.isConnected) return;
          el.style.transition = 'translate var(--motion-base) var(--ease-spring)';
          el.style.translate = '';
          el.addEventListener('transitionend', onDone);
          el.addEventListener('transitioncancel', onDone);
        })
      );
    }

    return () => {
      for (const id of rafIds) window.cancelAnimationFrame(id);
      for (const restore of restores) restore();
    };
    // Keyed on the rendered order string — the DOM is the dependency here.
  }, [orderKey]);

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
   * scrolled out of sight, the ▾ is how you reach them without scrubbing. The
   * measurement is a SHARED hook because the preview panel runs the same tab
   * language and must not have to re-derive the rule (card Z).
   */
  const showOverflowMenu = useTabStripOverflow(stripRef, tabs.length);

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
  // Named for what it IS, not how it is applied: it is applied as a MARGIN, so
  // that it sits outside this element's draggable border box (#74).
  const leftReserve = getSessionTitlePadding(isCompactSidebarOverlayOpen, reserveTitlebar);

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
      data-testid="chat-tab-strip-reserve"
      // THE RESERVE LIVES HERE, OUTSIDE THE SCROLL BOX — and that is the whole
      // point. It was padding-left on the strip itself, which is the scrolling
      // element: padding is part of the scrollable content, so the moment the
      // strip scrolled (selecting any tab scrolls it into view — see
      // handleSelect) the reserve travelled left with it and the tabs rode up
      // under the traffic lights and the floating controls. Measured at 800px
      // with 11 tabs: scrollLeft 428 put a tab at x=17, beneath lights that end
      // at x=72 and controls that span 100–164.
      //
      // The unit test passed throughout, because it asserted the property was
      // SET. It was set. It just could not hold: a reserve inside the thing that
      // moves is not a reserve. On the wrap it is a fixed gutter — the scroll
      // box now BEGINS at 172px and its overflow clips there, so no scroll
      // offset can ever carry a tab across it.
      //
      // MARGIN, NOT PADDING, AND THAT IS LOAD-BEARING (#74). This element is
      // `-webkit-app-region: drag`. Padding lives INSIDE the border box, so
      // with the reserve as padding the drag rect still began at x=0 and
      // covered the very titlebar controls the reserve exists to protect —
      // the OS ate every click on them, because Electron folds app-region
      // rects in TREE order (a later `drag` re-covers an earlier `no-drag`,
      // z-index irrelevant) and this wrap is mounted after them. A margin is
      // OUTSIDE the border box, so the drag rect now starts at the reserve
      // and the tabs land in exactly the same place as before.
      //
      // Both rects in this row had to move: measured with real CGEventPost
      // input, the header's rect and this one were EACH sufficient on their
      // own to kill the buttons, so fixing only one changed nothing. See the
      // matching note in BaseChat.tsx.
      //
      // Do not "simplify" this back to padding, and do not try to remove a
      // rect from the fold by writing `-webkit-app-region: none` — measured,
      // an explicitly specified `none` computes to `no-drag` in Blink (it
      // subtracts a rect); only an ABSENT declaration is truly absent.
      style={{ marginLeft: leftReserve, WebkitAppRegion: 'drag' } as CSSProperties}
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
        // The strip lives INSIDE the wrap's WebkitAppRegion:'drag' rect. R1 was
        // measured with real OS input through CDP: a pointer drag on a no-drag
        // child inside a drag region DOES reach the DOM and the window does not
        // move. So the strip may live here — but every tab must still declare
        // no-drag itself, or the OS eats the gesture before React sees it.
        //
        // This one keeps `drag` safely: it begins at the wrap's CONTENT edge,
        // which the wrap's margin has already pushed past the titlebar controls
        // (#74), so it never reaches them the way the wrap itself did.
        //
        // paddingLeft: 0 cancels `.br-tabstrip`'s `padding: 0 8px` on the left
        // only, so the reserve on the wrap is the whole left inset and the first
        // tab still lands exactly on it (172px, or 16px unreserved) rather than
        // 8px further right. The right half of the shorthand is untouched.
        style={{ paddingLeft: 0, WebkitAppRegion: 'drag' } as CSSProperties}
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
              // One attribute, two sources: this window's own drag, or a merge
              // arriving from another window. They are mutually exclusive in
              // time — a cross-window drag delivers no pointer events here, so
              // `dragOverTabId` is null for its whole duration.
              data-dropbefore={
                dragOverTabId === tab.tabId || remoteDropBeforeTabId === tab.tabId
                  ? 'true'
                  : undefined
              }
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
                // Astryx §4.3: ONE icon-label gap across the app, 8px. The 7px
                // here was the last arbitrary one in the strip — a document tab
                // and a nav row now measure the same distance from glyph to
                // word. `gap-2` is the token, not a magic number.
                className="flex min-w-0 flex-1 items-center gap-2 bg-transparent text-left"
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
                {tabAnnotations[tab.sessionId]?.badge === 'subagent' && (
                  <span className="ml-1 flex-none rounded bg-background-code px-1 text-[10px] text-text-subtle">
                    sub
                  </span>
                )}
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
                    className="hidden flex-none rounded-sm p-0.5 text-text-muted hover:bg-background-medium hover:text-text-default group-hover:block"
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
                  className="flex-none rounded-sm p-0.5 text-text-muted opacity-0 transition-opacity hover:bg-background-medium hover:text-text-default focus-visible:opacity-100 group-hover:opacity-100"
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
