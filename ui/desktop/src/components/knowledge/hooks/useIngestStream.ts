import { useCallback, useRef, useState } from 'react';
import { client } from '../../../api/client.gen';

export type SubAgentEvent =
  | { kind: 'step'; index: number; assistant_text: string }
  | { kind: 'tool_call'; name: string; args: unknown }
  | { kind: 'tool_result'; name: string; ok: boolean; summary: string }
  | { kind: 'done'; reason: string; final_text: string };

export interface StreamState {
  events: SubAgentEvent[];
  status: 'idle' | 'streaming' | 'done' | 'error';
  finalResult: unknown;
  error?: string;
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
  const start = useCallback(
    async (path: string, body: unknown): Promise<'done' | 'error'> => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      setState({ events: [], status: 'streaming', finalResult: null });

      // Build the full URL from the SDK client's configured baseUrl
      const cfg = client.getConfig();
      const baseUrl = (cfg.baseUrl as string | undefined) ?? '';
      const url = baseUrl.replace(/\/$/, '') + path;

      // Mirror the auth headers that the SDK client uses
      const cfgHeaders = cfg.headers as Record<string, string> | undefined;
      const xSecretKey = cfgHeaders?.['X-Secret-Key'] ?? '';

      try {
        const res = await fetch(url, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(xSecretKey ? { 'X-Secret-Key': xSecretKey } : {}),
          },
          body: JSON.stringify(body),
          signal: controller.signal,
        });

        if (!res.ok || !res.body) {
          throw new Error(`HTTP ${res.status}`);
        }

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
          if (s.status === 'streaming') return { ...s, status: 'done' };
          return s;
        });

        void terminalError; // used only inside setState above
        return terminalStatus;
      } catch (e) {
        if ((e as Error).name !== 'AbortError') {
          const msg = e instanceof Error ? e.message : String(e);
          setState((s) => ({ ...s, status: 'error', error: msg }));
          return 'error';
        }
        return 'done'; // aborted — treat as non-error for the caller
      }
    },
    [],
  );

  const abort = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  return { ...state, start, abort };
}
