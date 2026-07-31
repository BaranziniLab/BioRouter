/**
 * BR-71 §4.3: pure planner mapping one workspace command frame onto existing
 * ChatGroups reducer actions plus declarative side effects. NO dispatching, NO
 * window access here — the ChatGroupsProvider executes the plan. Pure so every
 * behavior (split refusal, focus etiquette, annotation) is unit-testable
 * against real reducer state.
 */
import { activeTabOf, findTabBySession, type ChatGroupsAction } from './chatGroupsReducer';
// `ChatGroupsState` lives in chatGroupsTypes — the reducer does not re-export it.
import type { ChatGroupsState } from './chatGroupsTypes';
import { MAX_GROUPS, groupCountOf } from './chatGroupsLayout';
import type { WorkspaceCommand, WorkspaceCommandResult } from './workspaceCommandRegistry';

export type TabAnnotation = { badge?: string; parentSessionId?: string };

export type WorkspaceCommandPlan = {
  result: WorkspaceCommandResult;
  /** Reducer actions to dispatch, in order. */
  actions: ChatGroupsAction[];
  /** Relay to the create-chat-window IPC (placement:"window" / open_window). */
  openWindowSessionId?: string;
  /** Surface a toast. */
  notify?: { message: string; level?: string };
  /** Record a tab annotation (subagent badge, parent link). */
  annotate?: { sessionId: string; annotation: TabAnnotation };
};

export function planWorkspaceCommand(
  cmd: WorkspaceCommand,
  state: ChatGroupsState
): WorkspaceCommandPlan {
  switch (cmd.cmd) {
    case 'open_tab': {
      if (!cmd.session_id) return refuse('missing session_id');
      // The pane count is a leaf count over the layout TREE. `groupCountOf` is
      // the same predicate `moveTabToGroup` itself uses to refuse a split
      // (chatGroupsReducer's moveTabToGroup), so the planner's refusal and the
      // reducer's cannot disagree.
      if (cmd.placement === 'split' && groupCountOf(state.layout) >= MAX_GROUPS) {
        return refuse(`split refused: already at ${MAX_GROUPS} groups`);
      }
      const previouslyActive = activeTabOf(state)?.tabId ?? null;
      const actions: ChatGroupsAction[] = [
        // Dedupe by session id is the reducer's own rule (openTab): "open or
        // focus session X" is this one dispatch.
        { type: 'openTab', payload: { sessionId: cmd.session_id } },
      ];
      if (cmd.placement === 'split') {
        const existing = findTabBySession(state, cmd.session_id);
        if (existing) {
          // Already-open session: move its tab into a new right-edge group.
          actions.push({
            type: 'moveTabToGroup',
            tabId: existing.tabId,
            targetGroupId: existing.groupId,
            zone: 'right',
          });
        }
        // A NEW session's tab id does not exist until the openTab commits; the
        // provider's executor performs the follow-up move against post-commit
        // state (see the executor, which re-plans the move).
      }
      if (cmd.focus === false && previouslyActive) {
        // §4.1 focus etiquette: background-open never steals the composer.
        actions.push({ type: 'activateTab', tabId: previouslyActive });
      }
      return {
        result: { ok: true, detail: cmd.placement === 'split' ? 'opened in split' : 'opened' },
        actions,
      };
    }
    case 'activate_tab': {
      const hit = cmd.session_id ? findTabBySession(state, cmd.session_id) : null;
      if (!hit) return refuse('session has no tab');
      return { result: { ok: true }, actions: [{ type: 'activateTab', tabId: hit.tabId }] };
    }
    case 'close_tab': {
      const hit = cmd.session_id ? findTabBySession(state, cmd.session_id) : null;
      if (!hit) return refuse('session has no tab');
      return { result: { ok: true }, actions: [{ type: 'closeTab', tabId: hit.tabId }] };
    }
    case 'open_window':
      if (!cmd.session_id) return refuse('missing session_id');
      return {
        result: { ok: true, detail: 'window requested' },
        actions: [],
        openWindowSessionId: cmd.session_id,
      };
    case 'notify':
      return {
        result: { ok: true },
        actions: [],
        notify: { message: cmd.message ?? 'Workspace notification', level: cmd.level },
      };
    case 'annotate_tab': {
      if (!cmd.session_id) return refuse('missing session_id');
      return {
        result: { ok: true },
        actions: [],
        annotate: {
          sessionId: cmd.session_id,
          annotation: { badge: cmd.badge, parentSessionId: cmd.parent_session_id },
        },
      };
    }
    default:
      return refuse(`unknown cmd '${(cmd as WorkspaceCommand).cmd}'`);
  }
}

function refuse(detail: string): WorkspaceCommandPlan {
  return { result: { ok: false, detail }, actions: [] };
}
