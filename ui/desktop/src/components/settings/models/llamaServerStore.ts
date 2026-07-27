/**
 * Module-level singleton store for the Llama Server (llama.cpp sidecar)
 * status and any in-flight install/start/warm-up operation.
 *
 * The backend download/start already runs detached from the HTTP client (the
 * sidecar is a process-global singleton in biorouterd), but the progress
 * polling used to live in component-local state in both the onboarding card
 * and the settings inventory — navigating away unmounted the component,
 * cleared the interval, and made an in-flight download look cancelled
 * (issue #34). This store owns the single poll interval and the latest
 * status, so the polling lifecycle is tied to the OPERATION, not to any
 * component mount. Components subscribe via `useSyncExternalStore` (the
 * `useLlamaServer` hook) and render live progress whenever they are mounted.
 *
 * The interval can never leak: it self-terminates on the same conditions the
 * old `waitForInstall` used — sidecar ready with the matching model (install
 * operations), sidecar error, or a 60-minute timeout — and `endOperation`
 * always stops it when a driving component flow finishes.
 */
import { useSyncExternalStore } from 'react';
import { llamacppStatus, type LlamaCppStatusResponse, type SidecarStatus } from '../../../api';

export const LLAMA_SERVER_POLL_INTERVAL_MS = 1500;
export const LLAMA_SERVER_OPERATION_TIMEOUT_MS = 60 * 60 * 1000;

export type LlamaServerOperationKind = 'install' | 'start' | 'warmup';

export interface LlamaServerOperation {
  kind: LlamaServerOperationKind;
  /** Catalog model name (or raw HF spec) the operation targets. */
  model: string;
  /** Latest human-readable progress line (compacted sidecar detail). */
  message: string | null;
  startedAt: number;
}

export interface LlamaServerSnapshot {
  status: LlamaCppStatusResponse | null;
  operation: LlamaServerOperation | null;
}

export const compactStatusMessage = (message: string) => {
  const compacted = message.replace(/\s+/g, ' ').trim();
  if (compacted.length <= 180) return compacted;
  return `${compacted.slice(0, 177)}...`;
};

interface ReadyWaiter {
  model: string;
  resolve: () => void;
  reject: (err: Error) => void;
}

let snapshot: LlamaServerSnapshot = { status: null, operation: null };
const listeners = new Set<() => void>();
let pollId: ReturnType<typeof setInterval> | null = null;
let waiters: ReadyWaiter[] = [];

function emit() {
  for (const listener of [...listeners]) listener();
}

function setSnapshot(next: LlamaServerSnapshot) {
  snapshot = next;
  emit();
}

function stopPolling() {
  if (pollId !== null) {
    clearInterval(pollId);
    pollId = null;
  }
}

function settleWaiters(settle: (waiter: ReadyWaiter) => void) {
  const pending = waiters;
  waiters = [];
  for (const waiter of pending) settle(waiter);
}

/** Terminal failure: stop polling, reject waiters, clear the operation. */
function failOperation(error: Error) {
  stopPolling();
  settleWaiters((w) => w.reject(error));
  if (snapshot.operation) setSnapshot({ ...snapshot, operation: null });
}

/** Terminal success (install operations): stop polling, clear the operation. */
function completeOperation() {
  stopPolling();
  settleWaiters((w) => w.resolve());
  if (snapshot.operation) setSnapshot({ ...snapshot, operation: null });
}

async function pollTick(): Promise<void> {
  if (!snapshot.operation) {
    stopPolling();
    return;
  }
  try {
    const res = await llamacppStatus({ throwOnError: true });
    // The operation may have ended while the request was in flight; still
    // record the fresh status, but skip the operation lifecycle checks.
    const op = snapshot.operation;
    const sidecar = res.data.sidecar;
    let nextOp = op;
    if (op && sidecar.detail) {
      const message = compactStatusMessage(sidecar.detail);
      if (message !== op.message) nextOp = { ...op, message };
    }
    setSnapshot({ status: res.data, operation: nextOp });
    if (!nextOp) {
      stopPolling();
      return;
    }
    if (sidecar.state === 'error') {
      failOperation(new Error(sidecar.detail || 'Llama Server failed to install model'));
      return;
    }
    if (sidecar.state === 'ready' && sidecar.model === nextOp.model) {
      if (nextOp.kind === 'install') {
        completeOperation();
      } else {
        // Start/warm-up flows keep polling for detail updates until their
        // driving HTTP call finishes (endOperation), errors, or times out.
        settleWaiters((w) => w.resolve());
      }
      return;
    }
    if (Date.now() - nextOp.startedAt > LLAMA_SERVER_OPERATION_TIMEOUT_MS) {
      failOperation(new Error('Timed out waiting for the local model install'));
    }
  } catch {
    // Transient polling errors are fine; keep polling unless timed out.
    const op = snapshot.operation;
    if (op && Date.now() - op.startedAt > LLAMA_SERVER_OPERATION_TIMEOUT_MS) {
      failOperation(new Error('Timed out waiting for the local model install'));
    }
  }
}

export const llamaServerStore = {
  subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      // Unsubscribing never stops the poll loop — the operation owns it.
      listeners.delete(listener);
    };
  },

  getSnapshot(): LlamaServerSnapshot {
    return snapshot;
  },

  /** One-shot status refresh (no polling). Throws on transport failure. */
  async refresh(): Promise<LlamaCppStatusResponse> {
    const res = await llamacppStatus({ throwOnError: true });
    setSnapshot({ ...snapshot, status: res.data });
    return res.data;
  },

  /** Record a status payload returned by another endpoint (ensure/delete). */
  applyStatus(status: LlamaCppStatusResponse) {
    setSnapshot({ ...snapshot, status });
  },

  /** Merge a bare sidecar snapshot (e.g. from the warm-up response). */
  applySidecar(sidecar: SidecarStatus) {
    if (!snapshot.status) return;
    setSnapshot({ ...snapshot, status: { ...snapshot.status, sidecar } });
  },

  /**
   * Begin an operation and (unless `poll: false`, used for Ollama-driven
   * pulls that report progress through `setOperationMessage`) start the
   * shared status poll loop. Replaces any operation already in flight.
   */
  beginOperation(
    kind: LlamaServerOperationKind,
    model: string,
    message: string | null = null,
    opts?: { poll?: boolean }
  ) {
    stopPolling();
    settleWaiters((w) => w.reject(new Error('Superseded by a newer Llama Server operation')));
    setSnapshot({
      ...snapshot,
      operation: { kind, model, message, startedAt: Date.now() },
    });
    if (opts?.poll !== false) {
      pollId = setInterval(() => {
        void pollTick();
      }, LLAMA_SERVER_POLL_INTERVAL_MS);
    }
  },

  /** Update the in-flight operation's progress message. */
  setOperationMessage(message: string) {
    const op = snapshot.operation;
    if (!op) return;
    const compacted = compactStatusMessage(message);
    if (compacted === op.message) return;
    setSnapshot({ ...snapshot, operation: { ...op, message: compacted } });
  },

  /** End the in-flight operation (driving flow finished or failed). */
  endOperation() {
    stopPolling();
    settleWaiters((w) => w.reject(new Error('Llama Server operation ended')));
    if (snapshot.operation) setSnapshot({ ...snapshot, operation: null });
  },

  /**
   * Resolve when the sidecar is ready with `model` loaded; reject on sidecar
   * error or after the 60-minute timeout — the same terminal conditions the
   * old component-local `waitForInstall` used. Requires an active polling
   * operation (see `beginOperation`).
   */
  waitForReady(model: string): Promise<void> {
    if (!snapshot.operation || pollId === null) {
      return Promise.reject(
        new Error('No Llama Server operation is polling; call beginOperation first')
      );
    }
    return new Promise<void>((resolve, reject) => {
      waiters.push({ model, resolve, reject });
    });
  },
};

/** Subscribe a component to the shared Llama Server snapshot. */
export function useLlamaServer(): LlamaServerSnapshot {
  return useSyncExternalStore(llamaServerStore.subscribe, llamaServerStore.getSnapshot);
}

/** Test-only: clear all module state (listeners, poller, snapshot). */
export function resetLlamaServerStoreForTests() {
  stopPolling();
  waiters = [];
  listeners.clear();
  snapshot = { status: null, operation: null };
}
