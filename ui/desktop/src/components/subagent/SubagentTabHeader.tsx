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
    <div
      className="border-b border-border-subtle bg-background-muted px-4 py-2 text-sm"
      data-testid={`subagent-header-${sessionId}`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="flex-none rounded-full bg-background-code px-2 py-0.5 text-xs">
          subagent
        </span>
        <span className="min-w-0 truncate text-text-subtle">
          spawned by{' '}
          <button className="underline" onClick={onOpenParent}>
            {parentSessionName ?? parentSessionId}
          </button>
        </span>
        {running && (
          <button
            className="ml-auto flex-none rounded border border-border-subtle px-2 py-0.5 text-xs"
            onClick={onStop}
            aria-label="Stop subagent"
          >
            Stop subagent
          </button>
        )}
      </div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-text-subtle">
        {extensions.map((name) => (
          <span key={name} className="rounded bg-background-code px-1.5 py-0.5">
            {name}
          </span>
        ))}
        {knowledgeBases.map((kb) => (
          <span key={kb} className="rounded bg-background-code px-1.5 py-0.5">
            {kb}
          </span>
        ))}
        {/* Only offered when there is something to disclose. The backend's
            `persist_spawn_context` is best-effort (a failure only warns) and
            sessions older than it have no record, so `spawnContext` really can
            be absent while the header itself is still worth showing — and a
            toggle that can only ever open onto nothing is a dead control. */}
        {spawnContext && (
          <button
            className="underline"
            onClick={() => setExpanded((e) => !e)}
            aria-expanded={expanded}
            aria-controls={spawnContextId}
          >
            spawn context
          </button>
        )}
      </div>
      {expanded && spawnContext && (
        <pre
          id={spawnContextId}
          className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-background-code p-2 text-xs"
        >
          {spawnContext}
        </pre>
      )}
    </div>
  );
}
