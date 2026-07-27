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
 * Lifecycle invariants (each guards against a real failure mode):
 *
 * - Every operation is identified by a monotonically increasing id returned
 *   from `beginOperation`. All mutations (`setOperationMessage`,
 *   `applyStatus`/`applySidecar` when scoped, `waitForReady`) and termination
 *   (`endOperation`) require that id and no-op when the operation has been
 *   superseded — a stale caller's `finally` can never kill its replacement.
 * - The 60-minute deadline is an INDEPENDENT `setTimeout`, armed for every
 *   operation (polling or not). It does not depend on poll ticks reaching a
 *   particular branch, so a warm-up that sits "ready" forever, a `poll:false`
 *   Ollama pull that stalls, or a status endpoint that hangs all still time
 *   out.
 * - Poll ticks are serialized: a tick whose status request is still in
 *   flight causes later ticks to be skipped, and a response that arrives
 *   after its operation was superseded is discarded wholesale (the id is
 *   re-validated after the await), so op A's stale data can never update or
 *   terminate op B.
 * - Terminal failures are retained in `snapshot.lastError` so a surface can
 *   toast them immediately instead of waiting for a separate in-flight HTTP
 *   call to reject; `claimErrorToast` makes that surfacing exactly-once
 *   across components and driving flows.
 * - Under Vite HMR, module re-evaluation disposes the old instance's timers
 *   and waiters (`import.meta.hot.dispose`), so a dev reload can't orphan an
 *   interval; production builds compile the guard away.
 */
import { useSyncExternalStore } from 'react';
import { llamacppStatus, type LlamaCppStatusResponse, type SidecarStatus } from '../../../api';

export const LLAMA_SERVER_POLL_INTERVAL_MS = 1500;
export const LLAMA_SERVER_OPERATION_TIMEOUT_MS = 60 * 60 * 1000;

export type LlamaServerOperationKind = 'install' | 'start' | 'warmup';

export interface LlamaServerOperation {
  /** Id returned by `beginOperation`; required for mutations/termination. */
  id: number;
  kind: LlamaServerOperationKind;
  /** Catalog model name (or raw HF spec) the operation targets. */
  model: string;
  /** Latest human-readable progress line (compacted sidecar detail). */
  message: string | null;
  startedAt: number;
}

/** A terminal operation failure, retained for immediate UI surfacing. */
export interface LlamaServerTerminalError {
  opId: number;
  kind: LlamaServerOperationKind;
  model: string;
  message: string;
  at: number;
}

export interface LlamaServerSnapshot {
  status: LlamaCppStatusResponse | null;
  operation: LlamaServerOperation | null;
  /**
   * The most recent terminal failure (sidecar error or timeout). Cleared by
   * the next `beginOperation`. Surfaces (toasts) should gate on
   * `claimErrorToast` so the error is reported exactly once.
   */
  lastError: LlamaServerTerminalError | null;
}

export const compactStatusMessage = (message: string) => {
  const compacted = message.replace(/\s+/g, ' ').trim();
  if (compacted.length <= 180) return compacted;
  return `${compacted.slice(0, 177)}...`;
};

const timeoutMessage = (kind: LlamaServerOperationKind) => {
  switch (kind) {
    case 'warmup':
      return 'Timed out waiting for the local model warm-up';
    case 'start':
      return 'Timed out waiting for the local model to start';
    default:
      return 'Timed out waiting for the local model install';
  }
};

interface ReadyWaiter {
  model: string;
  resolve: () => void;
  reject: (err: Error) => void;
}

let snapshot: LlamaServerSnapshot = { status: null, operation: null, lastError: null };
const listeners = new Set<() => void>();
let pollId: ReturnType<typeof setInterval> | null = null;
let deadlineId: ReturnType<typeof setTimeout> | null = null;
let waiters: ReadyWaiter[] = [];
/** Monotonic operation-id source; also acts as the generation token. */
let opSeq = 0;
/** Id of the active operation; 0 when idle. */
let currentOpId = 0;
/** Id of the operation with a status request in flight; 0 when none. */
let tickInFlightFor = 0;
/** Whether `snapshot.lastError` has been claimed for toasting. */
let lastErrorClaimed = false;

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

function clearDeadline() {
  if (deadlineId !== null) {
    clearTimeout(deadlineId);
    deadlineId = null;
  }
}

function settleWaiters(settle: (waiter: ReadyWaiter) => void) {
  const pending = waiters;
  waiters = [];
  for (const waiter of pending) settle(waiter);
}

/** Terminal failure: stop timers, reject waiters, clear + retain the error. */
function failOperation(opId: number, error: Error) {
  if (opId !== currentOpId) return;
  const op = snapshot.operation;
  currentOpId = 0;
  stopPolling();
  clearDeadline();
  settleWaiters((w) => w.reject(error));
  lastErrorClaimed = false;
  setSnapshot({
    ...snapshot,
    operation: null,
    lastError: {
      opId,
      kind: op?.kind ?? 'install',
      model: op?.model ?? '',
      message: error.message,
      at: Date.now(),
    },
  });
}

/** Terminal success (install operations): stop timers, clear the operation. */
function completeOperation(opId: number) {
  if (opId !== currentOpId) return;
  currentOpId = 0;
  stopPolling();
  clearDeadline();
  settleWaiters((w) => w.resolve());
  if (snapshot.operation) setSnapshot({ ...snapshot, operation: null });
}

async function pollTick(opId: number): Promise<void> {
  // Stale interval (superseded operation) — never touch shared state.
  if (opId !== currentOpId || !snapshot.operation) return;
  // Serialize ticks: while a status request is in flight, later interval
  // firings are skipped instead of piling up overlapping requests.
  if (tickInFlightFor === opId) return;
  tickInFlightFor = opId;
  try {
    const res = await llamacppStatus({ throwOnError: true });
    // The operation may have been superseded or terminated while the request
    // was in flight; a stale response must not update or terminate the newer
    // operation, so it is discarded wholesale.
    if (opId !== currentOpId) return;
    const op = snapshot.operation;
    if (!op) return;
    const sidecar = res.data.sidecar;
    let nextOp = op;
    if (sidecar.detail) {
      const message = compactStatusMessage(sidecar.detail);
      if (message !== op.message) nextOp = { ...op, message };
    }
    setSnapshot({ ...snapshot, status: res.data, operation: nextOp });
    if (sidecar.state === 'error') {
      failOperation(opId, new Error(sidecar.detail || 'Llama Server failed to install model'));
      return;
    }
    if (sidecar.state === 'ready' && sidecar.model === nextOp.model) {
      if (nextOp.kind === 'install') {
        completeOperation(opId);
      } else {
        // Start/warm-up flows keep polling for detail updates until their
        // driving HTTP call finishes (endOperation) or errors; the
        // independent deadline timer still bounds them, so a warm-up hung in
        // the "ready" state cannot poll forever.
        settleWaiters((w) => w.resolve());
      }
    }
    // Timeouts are owned by the independent deadline timer armed in
    // beginOperation — deliberately no Date.now() bookkeeping here.
  } catch {
    // Transient polling errors are fine; the deadline timer owns the timeout.
  } finally {
    if (tickInFlightFor === opId) tickInFlightFor = 0;
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

  /**
   * Record a status payload returned by another endpoint (ensure/delete).
   * When `opId` is given, the payload is dropped if that operation is no
   * longer current (a superseded flow's late response must not clobber the
   * replacement's fresher status).
   */
  applyStatus(status: LlamaCppStatusResponse, opId?: number) {
    if (opId !== undefined && opId !== currentOpId) return;
    setSnapshot({ ...snapshot, status });
  },

  /** Merge a bare sidecar snapshot (e.g. from the warm-up response). */
  applySidecar(sidecar: SidecarStatus, opId?: number) {
    if (opId !== undefined && opId !== currentOpId) return;
    if (!snapshot.status) return;
    setSnapshot({ ...snapshot, status: { ...snapshot.status, sidecar } });
  },

  /**
   * Begin an operation and (unless `poll: false`, used for Ollama-driven
   * pulls that report progress through `setOperationMessage`) start the
   * shared status poll loop. Replaces any operation already in flight.
   * Always arms the independent 60-minute deadline, so even poll-less
   * operations cannot stay busy forever.
   *
   * Returns the operation id; every later mutation and `endOperation` for
   * this operation must pass it back.
   */
  beginOperation(
    kind: LlamaServerOperationKind,
    model: string,
    message: string | null = null,
    opts?: { poll?: boolean }
  ): number {
    const opId = ++opSeq;
    currentOpId = opId;
    stopPolling();
    clearDeadline();
    settleWaiters((w) => w.reject(new Error('Superseded by a newer Llama Server operation')));
    setSnapshot({
      ...snapshot,
      operation: { id: opId, kind, model, message, startedAt: Date.now() },
      lastError: null,
    });
    deadlineId = setTimeout(() => {
      failOperation(opId, new Error(timeoutMessage(kind)));
    }, LLAMA_SERVER_OPERATION_TIMEOUT_MS);
    if (opts?.poll !== false) {
      pollId = setInterval(() => {
        void pollTick(opId);
      }, LLAMA_SERVER_POLL_INTERVAL_MS);
    }
    return opId;
  },

  /** Update the in-flight operation's progress message (scoped to `opId`). */
  setOperationMessage(opId: number, message: string) {
    if (opId !== currentOpId) return;
    const op = snapshot.operation;
    if (!op) return;
    const compacted = compactStatusMessage(message);
    if (compacted === op.message) return;
    setSnapshot({ ...snapshot, operation: { ...op, message: compacted } });
  },

  /**
   * End the operation identified by `opId` (its driving flow finished or
   * failed). A superseded or already-terminated caller is a no-op — it can
   * never stop the replacement operation or its timers. Returns whether this
   * call actually ended the current operation.
   */
  endOperation(opId: number): boolean {
    if (opId !== currentOpId) return false;
    currentOpId = 0;
    stopPolling();
    clearDeadline();
    settleWaiters((w) => w.reject(new Error('Llama Server operation ended')));
    if (snapshot.operation) setSnapshot({ ...snapshot, operation: null });
    return true;
  },

  /**
   * Claim the right to toast `snapshot.lastError` for `opId`. Returns true
   * exactly once per terminal error (across all mounted components and any
   * still-running driving flows), so the failure is surfaced immediately by
   * whoever sees it first and never twice.
   */
  claimErrorToast(opId: number): boolean {
    if (!snapshot.lastError || snapshot.lastError.opId !== opId) return false;
    if (lastErrorClaimed) return false;
    lastErrorClaimed = true;
    return true;
  },

  /**
   * Resolve when the sidecar is ready with `model` loaded; reject on sidecar
   * error, on the 60-minute deadline, or when the operation is superseded.
   * Requires the id of the active polling operation (see `beginOperation`).
   */
  waitForReady(model: string, opId: number): Promise<void> {
    if (opId !== currentOpId || !snapshot.operation || pollId === null) {
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

/** Test-only: clear all module state (listeners, timers, snapshot). */
export function resetLlamaServerStoreForTests() {
  stopPolling();
  clearDeadline();
  waiters = [];
  listeners.clear();
  opSeq = 0;
  currentOpId = 0;
  tickInFlightFor = 0;
  lastErrorClaimed = false;
  snapshot = { status: null, operation: null, lastError: null };
}

// Vite HMR: module re-evaluation would otherwise orphan the old instance's
// interval/deadline and strand its waiters alongside a fresh second store.
// `import.meta.hot` is undefined in production builds and under vitest, so
// this guard is compiled away outside the dev server.
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    stopPolling();
    clearDeadline();
    settleWaiters((w) => w.reject(new Error('Llama Server store was reloaded (HMR)')));
  });
}
