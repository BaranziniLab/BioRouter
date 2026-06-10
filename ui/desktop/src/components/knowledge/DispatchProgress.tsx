import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  LoaderCircle,
  StopCircle,
} from 'lucide-react';
import { useMemo, useState } from 'react';
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
    return 'Using the current staged source context.';
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

  return entries.length > 0 ? entries.join(' • ') : 'Using the current staged source context.';
}

function renderEventLine(ev: SubAgentEvent): { tone: string; text: string } {
  if (ev.kind === 'step') {
    return {
      tone: 'text-text-default',
      text: `Step ${ev.index + 1}: ${ev.assistant_text.trim() || 'Preparing the next knowledge-base action.'}`,
    };
  }

  if (ev.kind === 'tool_call') {
    return {
      tone: 'text-text-muted',
      text: `Tool call · ${prettyToolName(ev.name)} · ${summarizeArgs(ev.args)}`,
    };
  }

  if (ev.kind === 'tool_result') {
    return {
      tone: ev.ok ? 'text-text-success' : 'text-text-danger',
      text: `${ev.ok ? 'Completed' : 'Issue'} · ${prettyToolName(ev.name)} · ${
        ev.summary.trim() || (ev.ok ? 'Completed successfully.' : 'Returned an error.')
      }`,
    };
  }

  return {
    tone: 'text-text-muted',
    text:
      ev.reason === 'cancelled'
        ? 'Digest stopped before the current item completed.'
        : `Digest finished · ${ev.reason}`,
  };
}

interface Props {
  state: StreamState;
  onAbort?: () => void;
}

export function DispatchProgress({ state, onAbort }: Props) {
  const [open, setOpen] = useState(true);

  const statusLabel =
    state.status === 'starting'
      ? 'Preparing digest...'
      : state.status === 'streaming'
        ? 'Digesting...'
        : state.status === 'stopping'
          ? 'Stopping...'
          : state.status === 'done'
            ? 'Digest complete'
            : state.status === 'error'
              ? `Digest error: ${state.error}`
              : '';

  const lines = useMemo(() => state.events.map(renderEventLine), [state.events]);

  if (state.status === 'idle') return null;

  return (
    <div className="rounded-xl border border-border-subtle bg-background-surface">
      <div className="flex items-center justify-between gap-3 px-3 py-2.5">
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          className="flex min-w-0 flex-1 items-center gap-2 text-left text-xs font-medium"
        >
          {open ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          )}
          <span className="min-w-0 break-words">
            {statusLabel}
            <span className="ml-2 text-text-muted">{state.events.length} events</span>
          </span>
        </button>

        {(state.status === 'starting' ||
          state.status === 'streaming' ||
          state.status === 'stopping') &&
          onAbort && (
            <button
              type="button"
              onClick={onAbort}
              className="inline-flex shrink-0 items-center gap-1 text-xs text-text-muted transition-colors hover:text-text-danger"
              title="Stop digestion"
              disabled={state.status === 'stopping'}
            >
              {state.status === 'stopping' ? (
                <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <StopCircle className="h-3.5 w-3.5" />
              )}
              {state.status === 'stopping' ? 'Stopping...' : 'Stop'}
            </button>
          )}
      </div>

      {open && (
        <div className="border-t border-border-subtle px-3 py-3">
          <div className="max-h-[260px] space-y-2 overflow-y-auto text-[11px] leading-5">
            {lines.length === 0 && (
              <div className="flex items-start gap-2 text-text-muted">
                <LoaderCircle className="mt-0.5 h-3.5 w-3.5 shrink-0 animate-spin" />
                <p className="min-w-0 break-words whitespace-pre-wrap">
                  {state.status === 'starting'
                    ? 'Checking the staged source and opening the digest pipeline.'
                    : state.status === 'stopping'
                      ? 'Waiting for the current request to stop cleanly.'
                      : 'Waiting for tool-call updates.'}
                </p>
              </div>
            )}

            {lines.map((line, index) => (
              <div key={index} className={`flex items-start gap-2 ${line.tone}`}>
                {line.text.startsWith('Completed') ? (
                  <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                ) : line.text.startsWith('Issue') || line.text.startsWith('Digest error') ? (
                  <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                ) : (
                  <span className="mt-[7px] h-1.5 w-1.5 shrink-0 rounded-full bg-current opacity-60" />
                )}
                <p className="min-w-0 break-words whitespace-pre-wrap">{line.text}</p>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
