/**
 * BR-71 §4.3 renderer side: one WebSocket per window to GET /ui/workspace.
 * Inbound workspace frames are applied through workspaceCommandRegistry (the
 * ChatGroupsProvider registers the real handler); every frame carrying a
 * request_id is answered with workspace_result. Outbound: debounced
 * workspace_echo layout reports (sendEcho is handed back to the provider).
 *
 * Reconnects with backoff; the daemon side is generation-guarded, so a
 * reconnect simply claims a new generation (WorkspaceBridge.attach).
 */
import { useEffect, useMemo, useRef } from 'react';
import { getApiUrl } from '../config';
import {
  applyWorkspaceCommand,
  type WorkspaceCommand,
  type WorkspaceCommandResult,
} from '../components/chatGroups/workspaceCommandRegistry';
// State + layout types live in chatGroupsTypes, never in chatGroupsReducer —
// the reducer imports them and re-exports nothing.
import { leafGroupIds, type ChatGroupsState } from '../components/chatGroups/chatGroupsTypes';

type EchoLayoutGroup = {
  group_id: string;
  active_tab: string | null;
  tabs: { tab_id: string; session_id: string; title: string }[];
};

export type EchoFrame = {
  type: 'workspace_echo';
  window_id: string;
  focused_session: string | null;
  layout: EchoLayoutGroup[];
};

/** How long a burst of layout changes is allowed to settle before it is reported. */
const ECHO_DEBOUNCE_MS = 300;

/**
 * Pure: ChatGroups state → echo frame (unit-tested without a socket).
 *
 * The layout is a TREE (`GroupLayout`), never an ordered list — `ChatGroupsState`
 * has no `order` field and never had one. Strip order therefore comes from the
 * in-order leaf walk `leafGroupIds` (`chatGroupsTypes.ts`), which is exactly
 * what `ChatGroupsShell` renders.
 *
 * Do NOT reach for `Object.values(state.groups)`. It is the obvious substitute
 * and it is wrong: the record is keyed by INSERTION, and `splitLeaf` inserts a
 * left/top split's new group into the tree BEFORE the group it split while the
 * record still appends it last. This frame is the only thing the daemon's
 * `merged_layout()` / `focused_or_recent()` (Task 22) ever see, so getting the
 * order wrong mis-describes the user's window to every workspace tool, with no
 * error and no failing test anywhere else.
 */
export function buildEchoFrame(
  windowId: string,
  focusedSession: string | null,
  state: ChatGroupsState
): EchoFrame {
  return {
    type: 'workspace_echo',
    window_id: windowId,
    focused_session: focusedSession,
    layout: leafGroupIds(state.layout)
      .map((groupId) => state.groups[groupId])
      .filter(Boolean)
      .map((group) => ({
        group_id: group.groupId,
        active_tab: group.activeTabId,
        tabs: group.tabs.map((tab) => ({
          tab_id: tab.tabId,
          session_id: tab.sessionId,
          title: tab.title,
        })),
      })),
  };
}

export function useWorkspaceChannel({
  secret,
  windowId,
  enabled,
}: {
  secret: string | null;
  windowId: string;
  enabled: boolean;
}): { sendEcho: (echo: EchoFrame) => void } {
  const socketRef = useRef<WebSocket | null>(null);
  const echoTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastEcho = useRef<EchoFrame | null>(null);

  useEffect(() => {
    if (!enabled || !secret) return;
    let disposed = false;
    let retryMs = 1000;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      if (disposed) return;
      let ws: WebSocket;
      try {
        const base = getApiUrl('/ui/workspace').replace(/^http/, 'ws');
        ws = new WebSocket(
          `${base}?secret=${encodeURIComponent(secret)}&window_id=${encodeURIComponent(windowId)}`
        );
      } catch {
        // No usable endpoint (no appConfig yet, malformed host). Retrying is
        // harmless and the window keeps working without the channel.
        retryTimer = setTimeout(connect, retryMs);
        retryMs = Math.min(retryMs * 2, 15000);
        return;
      }
      socketRef.current = ws;
      ws.onopen = () => {
        retryMs = 1000;
        if (lastEcho.current) ws.send(JSON.stringify(lastEcho.current));
      };
      ws.onmessage = (event) => {
        let frame: WorkspaceCommand;
        try {
          frame = JSON.parse(String(event.data));
        } catch {
          return;
        }
        if (frame.type !== 'workspace') return;
        let result: WorkspaceCommandResult;
        try {
          result = applyWorkspaceCommand(frame);
        } catch (err) {
          // A frame carrying a request_id is a tool call PARKED on the reply
          // (WorkspaceBridge's pending map). Letting an exception escape here
          // does not fail that call — it hangs it, to whatever timeout the
          // daemon has, while this renderer carries on perfectly healthy. Turn
          // it into a refusal the daemon can read and report.
          result = { ok: false, detail: `renderer error: ${String(err)}` };
        }
        if (frame.request_id) {
          ws.send(
            JSON.stringify({
              type: 'workspace_result',
              request_id: frame.request_id,
              ok: result.ok,
              detail: result.detail ?? '',
            })
          );
        }
      };
      ws.onclose = () => {
        // Only if this socket is still the live one. `socketRef` is shared
        // across effect runs, and a close event is asynchronous: when the effect
        // re-runs (a new secret or window id) the cleanup closes the old socket
        // but its close event lands AFTER the replacement is installed. Nulling
        // unconditionally would clear a socket that is open, and every echo
        // after that is dropped in silence — with no reconnect, because nothing
        // actually closed.
        if (socketRef.current === ws) socketRef.current = null;
        if (!disposed) {
          retryTimer = setTimeout(connect, retryMs);
          retryMs = Math.min(retryMs * 2, 15000);
        }
      };
    };
    connect();
    return () => {
      disposed = true;
      if (retryTimer) clearTimeout(retryTimer);
      socketRef.current?.close();
    };
  }, [enabled, secret, windowId]);

  // A debounced echo must not outlive the provider: an unmounted window's timer
  // firing into a torn-down environment is a leak, and in tests it is a leak
  // that reports as an unrelated failure.
  useEffect(
    () => () => {
      if (echoTimer.current) clearTimeout(echoTimer.current);
      echoTimer.current = null;
    },
    []
  );

  return useMemo(
    () => ({
      sendEcho: (echo: EchoFrame) => {
        lastEcho.current = echo;
        if (echoTimer.current) clearTimeout(echoTimer.current);
        echoTimer.current = setTimeout(() => {
          echoTimer.current = null;
          const ws = socketRef.current;
          if (ws && ws.readyState === WebSocket.OPEN && lastEcho.current) {
            ws.send(JSON.stringify(lastEcho.current));
          }
        }, ECHO_DEBOUNCE_MS); // debounced (§4.3)
      },
    }),
    []
  );
}
