import { panelAccessFor } from '../artifacts/panelAccessRegistry';
import type { WorkspaceCommand, WorkspaceCommandResult } from './workspaceCommandRegistry';

/**
 * Answers the agent's two questions about the preview panel.
 *
 * Kept out of the tab planner deliberately: these change no layout, and routing
 * them through a planner that returns a plan of tab mutations would have meant
 * inventing a no-op plan shape for them.
 */

/** Bounded so a long document cannot swallow a turn's context. */
const MAX_TEXT_CHARS = 40_000;
const DEFAULT_TEXT_CHARS = 20_000;

export async function runPanelCommand(cmd: WorkspaceCommand): Promise<WorkspaceCommandResult> {
  const sessionId = cmd.session_id;
  if (!sessionId) return { ok: false, detail: 'no session_id given' };

  const panel = panelAccessFor(sessionId);
  if (!panel) {
    // A chat that is not on screen in this window has no panel to read. Say
    // which case it is: "closed" and "not open here" mean different things to
    // an agent deciding what to do next.
    return { ok: false, detail: 'this chat has no preview panel in this window' };
  }

  const descriptor = panel.describe();
  if (!descriptor.open) return { ok: false, detail: 'the preview panel is not open' };

  if (cmd.cmd === 'read_panel') {
    // A non-positive `max_chars` means "you decide", not "one character".
    // Clamping it up to 1 would have been the arithmetically obvious thing and
    // would answer a mistyped 0 with a single letter.
    const requested = cmd.max_chars && cmd.max_chars > 0 ? cmd.max_chars : DEFAULT_TEXT_CHARS;
    const limit = Math.min(requested, MAX_TEXT_CHARS);
    const snapshot = await panel.readText(limit);
    if (!snapshot) {
      return {
        ok: false,
        detail: `the panel is showing ${descriptor.kind ?? 'something'} with no readable text; use capture_panel to see it`,
        data: { panel: descriptor },
      };
    }
    return {
      ok: true,
      detail: `read ${snapshot.text.length} characters from ${snapshot.title}`,
      data: {
        panel: descriptor,
        content: snapshot.text,
        content_kind: snapshot.kind,
        locator: snapshot.locator ?? null,
        truncated: snapshot.truncated,
      },
    };
  }

  const shot = await panel.capture();
  if (!shot) {
    // `capturePage` returns an empty image rather than rejecting when the view
    // was hidden and then navigated, so this is a real outcome, not a bug.
    return { ok: false, detail: 'the panel could not be captured right now' };
  }
  // The path, never the bytes: the workspace channel caps an inbound frame at
  // 128 KiB and hands stored echoes to the model verbatim.
  return {
    ok: true,
    detail: `captured the preview panel to ${shot.path}`,
    data: { panel: descriptor, screenshot_path: shot.path },
  };
}
