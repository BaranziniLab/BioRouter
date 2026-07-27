import { useEffect } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { PairRouteState } from '../Pair';
import { useChatGroups } from '../../contexts/ChatGroupsContext';
import { leafGroupIds } from './chatGroupsTypes';
import { hasPendingNewTab } from './newTabRegistry';

/**
 * No tabs → Home (issue #38). The empty "New Session" pane is never the
 * resting state: when the ENTIRE layout has zero tabs and nothing is en route
 * to becoming one, /pair redirects to '/' (the Hub — heatmap + recent chats).
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
 *   - hasPendingNewTab(): Cmd+T from Settings. PEEK, never consume — the
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

  const hasState = state !== undefined;
  useEffect(() => {
    if (!hasState) return; // outside a provider there is nothing to judge
    if (tabCount > 0) return;
    if (resumeSessionId || isNewChat || initialMessage) return;
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
    workflowId,
    workflowDeeplink,
    isCreatingSession,
    navigate,
  ]);
}
