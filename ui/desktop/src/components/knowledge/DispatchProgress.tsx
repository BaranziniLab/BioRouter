import { ChevronDown, ChevronRight, StopCircle } from 'lucide-react';
import { useState } from 'react';
import type { StreamState, SubAgentEvent } from './hooks/useIngestStream';

function EventLine({ ev }: { ev: SubAgentEvent }) {
  if (ev.kind === 'step') {
    return (
      <>
        step {ev.index}: {ev.assistant_text.substring(0, 80)}
      </>
    );
  }
  if (ev.kind === 'tool_call') {
    return (
      <>
        → {ev.name}({JSON.stringify(ev.args).substring(0, 60)})
      </>
    );
  }
  if (ev.kind === 'tool_result') {
    return (
      <>
        ← {ev.name}: {ev.ok ? '✓' : '✗'} {ev.summary.substring(0, 60)}
      </>
    );
  }
  if (ev.kind === 'done') {
    return <>done: {ev.reason}</>;
  }
  return null;
}

interface Props {
  state: StreamState;
  onAbort?: () => void;
}

export function DispatchProgress({ state, onAbort }: Props) {
  const [open, setOpen] = useState(true);

  if (state.status === 'idle') return null;

  const statusLabel =
    state.status === 'streaming'
      ? 'Digesting…'
      : state.status === 'done'
        ? 'Done'
        : state.status === 'error'
          ? `Error: ${state.error}`
          : '';

  return (
    <div className="border border-border-subtle rounded-xl bg-background-surface">
      <div className="w-full flex items-center justify-between px-3 py-2">
        <button
          onClick={() => setOpen((o) => !o)}
          className="flex items-center gap-2 text-xs font-medium"
        >
          <span>
            {statusLabel}
            <span className="ml-2 text-text-muted">{state.events.length} steps</span>
          </span>
          {open ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        </button>
        {state.status === 'streaming' && onAbort && (
          <button
            onClick={onAbort}
            className="text-xs text-text-muted hover:text-red-500 inline-flex items-center gap-1"
            title="Stop digestion"
          >
            <StopCircle className="w-3 h-3" /> Stop
          </button>
        )}
      </div>

      {open && (
        <div className="border-t border-border-subtle px-3 py-2 max-h-[240px] overflow-y-auto flex flex-col gap-1.5">
          {state.events.length === 0 && (
            <div className="text-[10px] font-mono text-text-muted">Waiting for events…</div>
          )}
          {state.events.map((ev, i) => (
            <div key={i} className="text-[10px] font-mono text-text-muted">
              <EventLine ev={ev} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
