/**
 * BR-71 §4.5: the glass-box header on a subagent's tab. Shows the spawned-by
 * link, the child's grants (extensions from GET /sessions/{id}/extensions, KBs
 * from the spawn-context record — both fetched by the mounting container), and the exact spawn
 * context (the provenance:spawn_context first message), expandable. Stop
 * resolves the parent's tool call as Incomplete (the backend path — the button
 * merely posts /agent/cancel via onStop). Closing the tab never kills the
 * child; Stop is the only kill switch here.
 */
import { useState } from 'react';
import { Button } from '../ui/button';

export function SubagentTabHeader({
  sessionId,
  parentSessionId,
  parentSessionName,
  spawnContext,
  extensions,
  knowledgeBases,
  running,
  onOpenParent,
  onStop,
}: {
  sessionId: string;
  parentSessionId: string;
  parentSessionName?: string;
  spawnContext?: string;
  extensions: string[];
  knowledgeBases: string[];
  running: boolean;
  onOpenParent: () => void;
  onStop: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const spawnContextId = `subagent-spawn-context-${sessionId}`;
  return (
    <>
      <div
      /*
       * ⚠ One band's worth of height, on the chat's own ground.
       *
       * This used to be `bg-background-muted` with content-driven height — a
       * THIRD ground beside the header's `bg-sidebar` and the transcript's
       * canvas, growing taller with every grant because the chip row below was
       * an uncapped flex-wrap of one chip per extension and per knowledge base.
       * A subagent tab was visibly taller than an ordinary one, and the taller
       * it got the more it looked like a debug panel rather than a chat.
       *
       * The ordinary chat already settled this question: counts in the
       * composer's row 1, names behind a popover, never a list in a header
       * (`ChatInput.tsx:2668-2670` records the decision). This now follows that
       * convention.
       */
      className="flex h-chrome items-center gap-2 border-b border-border-subtle bg-sidebar px-4 text-secondary"
      data-testid={`subagent-header-${sessionId}`}
    >
      {/* The tab strip already badges this tab as a subagent, so the standalone
          "subagent" pill said the same thing twice on one screen. */}
      <span className="min-w-0 truncate text-text-subtle">
        spawned by{' '}
        <button className="underline" onClick={onOpenParent}>
          {parentSessionName ?? parentSessionId}
        </button>
      </span>
      {/* Counts, not names. The names are one click away and the count is the
          part that survives a glance — the same trade the composer's three
          chips make. `title` carries the full list so it is still reachable
          without opening anything. */}
      {extensions.length > 0 && (
        <span
          className="flex-none rounded bg-background-code px-1.5 py-0.5 text-supporting text-text-subtle"
          title={extensions.join(', ')}
        >
          {extensions.length} extension{extensions.length === 1 ? '' : 's'}
        </span>
      )}
      {knowledgeBases.length > 0 && (
        <span
          className="flex-none rounded bg-background-code px-1.5 py-0.5 text-supporting text-text-subtle"
          title={knowledgeBases.join(', ')}
        >
          {knowledgeBases.length} knowledge base{knowledgeBases.length === 1 ? '' : 's'}
        </span>
      )}
      {/* Only offered when there is something to disclose. The backend's
          `persist_spawn_context` is best-effort (a failure only warns) and
          sessions older than it have no record, so `spawnContext` really can
          be absent while the header itself is still worth showing — and a
          toggle that can only ever open onto nothing is a dead control. */}
      {spawnContext && (
        <button
          className="flex-none underline"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
          aria-controls={spawnContextId}
        >
          spawn context
        </button>
      )}
      {running && (
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto flex-none"
            onClick={onStop}
            aria-label="Stop subagent"
          >
            Stop subagent
          </Button>
        )}
      </div>
      {/* Below the band, not inside it: the band is a fixed `h-chrome` so it
          stays exactly as tall as the ordinary chat header whether this is open
          or shut. */}
      {expanded && spawnContext && (
        <pre
          id={spawnContextId}
          className="max-h-64 overflow-auto whitespace-pre-wrap break-words border-b border-border-subtle bg-background-code p-2 text-supporting"
        >
          {spawnContext}
        </pre>
      )}
    </>
  );
}
