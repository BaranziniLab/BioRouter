import { useEffect } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { PairRouteState } from '../Pair';
import { useChatGroups } from '../../contexts/ChatGroupsContext';
import { leafGroupIds } from './chatGroupsTypes';
import { hasPendingNewTab } from './newTabRegistry';

/**
 * No tabs → Home (issue #38). The empty "New chat" pane is never the
 * resting state: when the ENTIRE layout has zero tabs and nothing is en route
 * to becoming one, /pair redirects to '/' (the Hub — greeting + usage heatmap).
 *
 * One effect covers both roads there: closing the last tab (the state update
 * re-renders /pair with zero tabs) and arriving on /pair via a stale deep
 * link. A split whose one half empties never gets here — collapseEmptyGroup
 * removes that leaf and the survivor keeps the layout non-empty. The reducer
 * is untouched: sessions are still never deleted, split halves still collapse
 * in-tree; only the route-level resting state changed.
 *
 * EVERY gate below is load-bearing, because this hook runs in a CHILD of
 * ChatGroupsProvider — its effect fires BEFORE the provider's mount effects
 * (URL-open, pending-Cmd+T consumption) can turn cargo into a tab:
 *
 *   - resumeSessionId (URL or route state): a deep link / Hub submit / diverge
 *     window the provider is about to open.
 *   - isNewChat: the sidebar's new-chat navigation; PairRouteContent opens the
 *     blank tab in its own effect.
 *   - initialMessage / workflowId / workflowDeeplink / isCreatingSession: the
 *     create-session path — cargo with no session yet; PairRouteContent is
 *     mid-flight creating one (isCreatingSession is still false in the very
 *     commit that starts the creation, so the raw inputs must gate too).
 *   - initialMessagePending (URL): a FRESH WINDOW born for a launcher message.
 *     That cargo cannot ride the URL — main.ts parks it in the main process
 *     and delivers it as `set-initial-message` IPC only after App.tsx signals
 *     react-ready, an effect that runs AFTER this child hook's. Without the
 *     marker the window's very first commit is a bare zero-tab /pair and the
 *     redirect bounces it Home before the message ever arrives. main.ts sets
 *     the param at window creation; the set-initial-message handler's
 *     navigation (with real route state) drops it. If session creation fails
 *     the marker keeps the window parked on the empty pane — the pre-#38
 *     resting state — rather than silently discarding the launch intent.
 *   - hasPendingNewTab(): Cmd+T from Settings, or a Cmd+T dispatched to a live
 *     provider whose tab has not COMMITTED yet. PEEK, never consume — the
 *     provider's consume-once effect runs after this one and must still find
 *     the request (see newTabRegistry).
 *
 * The redirect replaces (not pushes): the empty /pair entry is a dead end
 * nobody should Back into. Note that navigating away unmounts
 * TerminalDockProvider, so a terminal opened on the empty pane dies with the
 * redirect — consistent with Cmd+W's ladder, where the dock closes before the
 * surface does (#21).
 */
export function useEmptyPairRedirect(isCreatingSession: boolean): void {
  const groups = useChatGroups();
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();

  const state = groups?.state;
  const tabCount = state
    ? leafGroupIds(state.layout).reduce((count, id) => count + state.groups[id].tabs.length, 0)
    : 0;

  const routeState = (location.state as PairRouteState) || {};
  const resumeSessionId = searchParams.get('resumeSessionId') ?? routeState.resumeSessionId;
  const workflowId = searchParams.get('workflowId');
  const workflowDeeplink = window.appConfig?.get('workflowDeeplink') as string | undefined;
  const isNewChat = routeState.newChat === true;
  const initialMessage = routeState.initialMessage;
  // Set by main.ts on a fresh window whose launcher message is still parked in
  // the main process (see the docblock) — the cargo exists, just not here yet.
  const initialMessagePending = searchParams.get('initialMessagePending') !== null;

  const hasState = state !== undefined;
  useEffect(() => {
    if (!hasState) return; // outside a provider there is nothing to judge
    if (tabCount > 0) return;
    if (resumeSessionId || isNewChat || initialMessage) return;
    if (initialMessagePending) return;
    if (workflowId || workflowDeeplink) return;
    if (isCreatingSession) return;
    if (hasPendingNewTab()) return;
    navigate('/', { replace: true });
  }, [
    hasState,
    tabCount,
    resumeSessionId,
    isNewChat,
    initialMessage,
    initialMessagePending,
    workflowId,
    workflowDeeplink,
    isCreatingSession,
    navigate,
  ]);
}
