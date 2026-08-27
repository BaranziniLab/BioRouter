/**
 * BR-71 §4.3: pure planner mapping one workspace command frame onto existing
 * ChatGroups reducer actions plus declarative side effects. NO dispatching, NO
 * window access here — the ChatGroupsProvider executes the plan. Pure so every
 * behavior (split refusal, focus etiquette, annotation) is unit-testable
 * against real reducer state.
 */
import {
  activeTabOf,
  chatGroupsReducer,
  findTabBySession,
  type ChatGroupsAction,
} from './chatGroupsReducer';
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
      // Dedupe by session id is the reducer's own rule (openTab): "open or
      // focus session X" is this one dispatch.
      const open: ChatGroupsAction = {
        type: 'openTab',
        payload: { sessionId: cmd.session_id },
      };
      const actions: ChatGroupsAction[] = [open];

      let splits = false;
      if (cmd.placement === 'split') {
        // A NEW session's tab id does not exist until `openTab` has been
        // applied — so APPLY it, here, and read the id off the result.
        //
        // The planner and the reducer are both pure, so this simulation is
        // exact: the executor dispatches these same actions onto this same
        // state, in this order. That is the whole point — the move has to be in
        // the SAME plan (and so the same React batch) as the open. The first
        // implementation instead left the move to a `queueMicrotask` in the
        // provider that re-read `stateRef`; a frame delivered on a macrotask
        // (which is how ws.onmessage delivers every real frame) drains that
        // microtask BEFORE React commits, so the ref was pre-open, the lookup
        // returned null, the move was silently dropped — and the daemon was
        // told `ok: true, detail: 'opened in split'` for a window that never
        // split.
        const opened = chatGroupsReducer(state, open);
        const hit = findTabBySession(opened, cmd.session_id);
        if (hit) {
          const move: ChatGroupsAction = {
            // Split the tab off its OWN group: a new right-edge pane.
            type: 'moveTabToGroup',
            tabId: hit.tabId,
            targetGroupId: hit.groupId,
            zone: 'right',
          };
          // The reducer refuses splits of its own accord — a group's only tab
          // cannot be split off it (`moveTabToGroup`: `source.tabs.length <= 1`).
          // Ask it rather than assume, so `detail` describes what the user will
          // actually see and a pointless move never reaches the dispatch loop.
          splits =
            groupCountOf(chatGroupsReducer(opened, move).layout) > groupCountOf(opened.layout);
          if (splits) actions.push(move);
        }
      }
      if (cmd.focus === false && previouslyActive) {
        // §4.1 focus etiquette: background-open never steals the composer.
        actions.push({ type: 'activateTab', tabId: previouslyActive });
      }
      return {
        result: { ok: true, detail: splits ? 'opened in split' : 'opened' },
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
    // BR-71 §3c: something was written into this session from elsewhere, so
    // whatever tab is showing it needs a live feed. Purely a request to ATTACH
    // — no reducer action, no annotation, no focus steal — because the tab is
    // already where the user put it and the daemon has no business moving it.
    //
    // Refusing when the session has no tab is not a failure the caller should
    // act on: an injection into a conversation nobody has open is the ordinary
    // case. The detail says so rather than reading as an error.
    case 'observe': {
      if (!cmd.session_id) return refuse('missing session_id');
      const hit = findTabBySession(state, cmd.session_id);
      if (!hit) return { result: { ok: true, detail: 'no tab in this window' }, actions: [] };
      return { result: { ok: true }, actions: [] };
    }
    default:
      return refuse(`unknown cmd '${(cmd as WorkspaceCommand).cmd}'`);
  }
}

function refuse(detail: string): WorkspaceCommandPlan {
  return { result: { ok: false, detail }, actions: [] };
}
