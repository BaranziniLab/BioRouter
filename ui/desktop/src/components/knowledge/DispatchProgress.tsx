import {
  AlertCircle,
  Bot,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Hammer,
  LoaderCircle,
  StopCircle,
} from 'lucide-react';
import { useState } from 'react';
import type { StreamState, SubAgentEvent } from './hooks/useIngestStream';

function prettyToolName(name: string): string {
  const raw = name.startsWith('kb_') ? name.slice(3) : name;
  return raw
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function summarizeArgs(args: unknown): string {
  if (!args || typeof args !== 'object' || Array.isArray(args)) {
    return 'Running with the current source context';
  }

  const entries = Object.entries(args as Record<string, unknown>)
    .filter(([, value]) => value !== undefined && value !== null && value !== '')
    .slice(0, 3)
    .map(([key, value]) => {
      const rendered =
        typeof value === 'string'
          ? value
          : typeof value === 'number' || typeof value === 'boolean'
            ? String(value)
            : JSON.stringify(value);
      return `${key}: ${rendered}`;
    });

  return entries.length > 0 ? entries.join(' • ') : 'Running with the current source context';
}

function EventCard({ ev }: { ev: SubAgentEvent }) {
  if (ev.kind === 'step') {
    return (
      <div className="rounded-xl border border-border-subtle bg-background-default px-3 py-2">
        <div className="flex items-start gap-2">
          <Bot className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" />
          <div className="min-w-0">
            <div className="text-[11px] font-medium text-text-default">Planner step {ev.index + 1}</div>
            <p className="mt-1 text-[11px] leading-5 text-text-muted">
              {ev.assistant_text.trim() || 'Preparing the next knowledge-base action.'}
            </p>
          </div>
        </div>
      </div>
    );
  }

  if (ev.kind === 'tool_call') {
    return (
      <div className="rounded-xl border border-border-subtle bg-background-default px-3 py-2">
        <div className="flex items-start gap-2">
          <Hammer className="mt-0.5 h-4 w-4 shrink-0 text-text-accent" />
          <div className="min-w-0">
            <div className="text-[11px] font-medium text-text-default">{prettyToolName(ev.name)}</div>
            <p className="mt-1 text-[11px] leading-5 text-text-muted">{summarizeArgs(ev.args)}</p>
          </div>
        </div>
      </div>
    );
  }

  if (ev.kind === 'tool_result') {
    return (
      <div
        className={`rounded-xl border px-3 py-2 ${
          ev.ok
            ? 'border-emerald-200/70 bg-emerald-50/60 dark:border-emerald-900/40 dark:bg-emerald-950/20'
            : 'border-red-200/70 bg-red-50/60 dark:border-red-900/40 dark:bg-red-950/20'
        }`}
      >
        <div className="flex items-start gap-2">
          {ev.ok ? (
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
          ) : (
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-600 dark:text-red-400" />
          )}
          <div className="min-w-0">
            <div className="text-[11px] font-medium text-text-default">{prettyToolName(ev.name)}</div>
            <p className="mt-1 text-[11px] leading-5 text-text-muted">
              {ev.summary.trim() || (ev.ok ? 'Completed successfully.' : 'Returned an error.')}
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-border-subtle bg-background-default px-3 py-2">
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" />
        <div className="min-w-0">
          <div className="text-[11px] font-medium text-text-default">Digest finished</div>
          <p className="mt-1 text-[11px] leading-5 text-text-muted">
            {ev.reason === 'cancelled' ? 'Stopped before the current item completed.' : ev.reason}
          </p>
        </div>
      </div>
    </div>
  );
}

interface Props {
  state: StreamState;
  onAbort?: () => void;
}

export function DispatchProgress({ state, onAbort }: Props) {
  const [open, setOpen] = useState(true);

  if (state.status === 'idle') return null;

  const statusLabel =
    state.status === 'starting'
      ? 'Preparing digest…'
      : state.status === 'streaming'
      ? 'Digesting…'
      : state.status === 'stopping'
        ? 'Stopping…'
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
            <span className="ml-2 text-text-muted">{state.events.length} events</span>
          </span>
          {open ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        </button>
        {(state.status === 'starting' || state.status === 'streaming' || state.status === 'stopping') &&
          onAbort && (
          <button
            onClick={onAbort}
            className="text-xs text-text-muted hover:text-red-500 inline-flex items-center gap-1"
            title="Stop digestion"
            disabled={state.status === 'stopping'}
          >
            {state.status === 'stopping' ? (
              <LoaderCircle className="w-3 h-3 animate-spin" />
            ) : (
              <StopCircle className="w-3 h-3" />
            )}{' '}
            {state.status === 'stopping' ? 'Stopping…' : 'Stop'}
          </button>
        )}
      </div>

      {open && (
        <div className="border-t border-border-subtle px-3 py-2 max-h-[260px] overflow-y-auto flex flex-col gap-2">
          {state.events.length === 0 && (
            <div className="rounded-xl border border-border-subtle bg-background-default px-3 py-2 text-[11px] text-text-muted">
              {state.status === 'starting'
                ? 'Checking the upload and opening the digestion pipeline…'
                : state.status === 'stopping'
                  ? 'Waiting for the current request to stop cleanly…'
                  : 'Waiting for tool events…'}
            </div>
          )}
          {state.events.map((ev, i) => (
            <EventCard key={i} ev={ev} />
          ))}
        </div>
      )}
    </div>
  );
}
