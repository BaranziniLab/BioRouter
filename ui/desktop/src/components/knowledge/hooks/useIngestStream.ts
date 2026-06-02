import { useCallback, useRef, useState } from 'react';
import { client } from '../../../api/client.gen';

async function getSecretKey(): Promise<string> {
  // Match the renderer.tsx pattern — read directly from the Electron preload bridge.
  const w = window as unknown as { electron?: { getSecretKey?: () => Promise<string> } };
  if (w.electron?.getSecretKey) {
    try {
      return await w.electron.getSecretKey();
    } catch {
      return '';
    }
  }
  return '';
}

export type SubAgentEvent =
  | { kind: 'step'; index: number; assistant_text: string }
  | { kind: 'tool_call'; name: string; args: unknown }
  | { kind: 'tool_result'; name: string; ok: boolean; summary: string }
  | { kind: 'done'; reason: string; final_text: string };

export interface StreamState {
  events: SubAgentEvent[];
  status: 'idle' | 'starting' | 'streaming' | 'stopping' | 'done' | 'error';
  finalResult: unknown;
  error?: string;
}

export interface StreamRunResult {
  status: 'done' | 'error' | 'aborted';
  error?: string;
}

function extractErrorMessage(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';

  try {
    const parsed = JSON.parse(trimmed) as { message?: string; error?: string };
    return parsed.message ?? parsed.error ?? trimmed;
  } catch {
    return trimmed;
  }
}

/**
 * SSE ingest stream hook.
 *
 * Uses raw fetch + ReadableStream because EventSource does not support POST bodies.
 * The backend emits `data: {json}\n\n` blocks, with terminal `event: done` or `event: error`.
 */
export function useIngestStream() {
  const [state, setState] = useState<StreamState>({
    events: [],
    status: 'idle',
    finalResult: null,
  });
  const abortRef = useRef<AbortController | null>(null);

  /**
   * Start the SSE stream. Resolves when the stream closes.
   * Returns the terminal status so callers don't need to read stale state.
   */
  const runStream = useCallback(
    async (
      path: string,
      requestInit: Pick<RequestInit, 'body' | 'headers'>,
    ): Promise<StreamRunResult> => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      setState({ events: [], status: 'starting', finalResult: null });

      // Build the full URL from the SDK client's configured baseUrl
      const cfg = client.getConfig();
      const baseUrl = (cfg.baseUrl as string | undefined) ?? '';
      const url = baseUrl.replace(/\/$/, '') + path;

      // Read the secret key directly from the Electron preload bridge,
      // matching how renderer.tsx sets up the API client. The cast
      // `cfg.headers as Record<string,string>` is unreliable because
      // HeadersInit can be a Headers instance or a [string,string][] array.
      const xSecretKey = await getSecretKey();

      try {
        const res = await fetch(url, {
          method: 'POST',
          headers: {
            'X-Secret-Key': xSecretKey,
            ...(requestInit.headers ?? {}),
          },
          body: requestInit.body,
          signal: controller.signal,
        });

        if (!res.ok || !res.body) {
          const bodyText = await res.text().catch(() => '');
          const details = extractErrorMessage(bodyText);
          throw new Error(details ? `HTTP ${res.status}: ${details}` : `HTTP ${res.status}`);
        }

        setState((s) => ({ ...s, status: s.status === 'stopping' ? 'stopping' : 'streaming' }));

        const reader = res.body.getReader();
        const decoder = new window.TextDecoder();
        let buf = '';
        let terminalStatus: 'done' | 'error' = 'done';
        let terminalError: string | undefined;

        outer: while (true) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });
          const blocks = buf.split('\n\n');
          buf = blocks.pop() ?? '';

          for (const block of blocks) {
            const lines = block.split('\n');
            let eventName = 'message';
            let data = '';

            for (const line of lines) {
              if (line.startsWith('event: ')) eventName = line.substring(7).trim();
              else if (line.startsWith('data: ')) data += line.substring(6);
            }

            if (eventName === 'done') {
              const parsed = data ? (JSON.parse(data) as unknown) : null;
              setState((s) => ({ ...s, status: 'done', finalResult: parsed }));
              terminalStatus = 'done';
              break outer;
            } else if (eventName === 'error') {
              const parsed = data
                ? (JSON.parse(data) as { message?: string })
                : { message: 'unknown' };
              const msg = parsed.message ?? 'unknown';
              setState((s) => ({ ...s, status: 'error', error: msg }));
              terminalStatus = 'error';
              terminalError = msg;
              break outer;
            } else if (data) {
              try {
                const ev = JSON.parse(data) as SubAgentEvent;
                setState((s) => ({ ...s, events: [...s.events, ev] }));
              } catch {
                // ignore malformed SSE data frames
              }
            }
          }
        }

        // If the stream closed without an explicit done/error event, mark done
        setState((s) => {
          if (s.status === 'starting' || s.status === 'streaming' || s.status === 'stopping') {
            return { ...s, status: 'done' };
          }
          return s;
        });

        return { status: terminalStatus, error: terminalError };
      } catch (e) {
        if ((e as Error).name !== 'AbortError') {
          const msg = e instanceof Error ? e.message : String(e);
          setState((s) => ({ ...s, status: 'error', error: msg }));
          return { status: 'error', error: msg };
        }
        setState((s) => ({
          ...s,
          status: 'done',
          finalResult: { reason: 'cancelled' },
          events:
            s.events[s.events.length - 1]?.kind === 'done'
              ? s.events
              : [...s.events, { kind: 'done', reason: 'cancelled', final_text: '' }],
        }));
        return { status: 'aborted' };
      }
    },
    [],
  );

  const start = useCallback(
    async (path: string, body: unknown): Promise<StreamRunResult> =>
      runStream(path, {
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(body),
      }),
    [runStream]
  );

  const startMultipart = useCallback(
    async (path: string, formData: FormData): Promise<StreamRunResult> =>
      runStream(path, {
        body: formData,
      }),
    [runStream]
  );

  const abort = useCallback(() => {
    setState((s) => {
      if (s.status === 'idle' || s.status === 'done' || s.status === 'error') {
        return s;
      }
      return { ...s, status: 'stopping' };
    });
    abortRef.current?.abort();
  }, []);

  return { ...state, start, startMultipart, abort };
}
