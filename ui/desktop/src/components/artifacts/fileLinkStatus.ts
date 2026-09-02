import { useCallback, useEffect, useSyncExternalStore } from 'react';

/**
 * Does the file a chat link points at actually exist?
 *
 * The assistant names paths it never wrote (`something.py` it only *described*,
 * a `/tmp/...` scratch directory that has since been cleaned up), and the
 * renderer used to paint every one of them as an accent-coloured link that did
 * nothing when clicked. This module answers the one question that separates the
 * two cases, over a batched main-process `stat`.
 *
 * ## The four states, and why "unknown" is not one bucket
 *
 * A dead path must never be clickable, **not even for one frame** — a link that
 * is orange on first paint and grey a tick later is the bug this fixes, wearing
 * a shorter timescale. So the default is the plain, non-clickable rendering and
 * a confirmed hit *upgrades* it to a link.
 *
 * That rule cannot apply where there is no main process to ask: the browser
 * surface (`biorouter serve`, whose `window.electron` shim in `renderer.tsx`
 * carries no `checkFilePaths`) and every vitest suite that renders
 * `MarkdownContent` without an Electron bridge. On those surfaces "start plain"
 * would mean *nothing is ever a link again*. So the absence of the bridge is its
 * own state — {@link FileLinkExistence.unchecked} — and it keeps the
 * pre-existing contract of linking everything. The distinction is deliberate:
 *
 * - `unchecked`  we know nothing: no bridge, or the check itself failed.
 *                Legacy behaviour, link it.
 * - `checking`   bridge present, answer not back yet. Plain text.
 * - `present`    confirmed on disk (and inside the preview allowlist). Link it.
 * - `missing`    we asked and it is gone or denied. Plain text.
 *
 * `missing` is an ANSWER; `unchecked` is the lack of one. A failed call takes
 * the second, so a transient hiccup never de-links files that are really there.
 */
export type FileLinkExistence = 'unchecked' | 'checking' | 'present' | 'missing';

/** One entry of a batched existence check. Mirrors the type in `preload.ts`. */
export type FilePathCheckRequest = { path: string; workingDir?: string };
/** The whole answer: never contents, never a directory listing. */
export type FilePathCheckResult = { exists: boolean; isDirectory: boolean };

type CheckFilePaths = (requests: FilePathCheckRequest[]) => Promise<FilePathCheckResult[]>;

/** Only `present` and `unchecked` may render as a clickable link. */
export function isOpenableFileLink(existence: FileLinkExistence): boolean {
  return existence === 'present' || existence === 'unchecked';
}

function checkBridge(): CheckFilePaths | undefined {
  const candidate = window.electron?.checkFilePaths;
  return typeof candidate === 'function' ? candidate : undefined;
}

/** Whether this surface can answer the question at all. */
export function fileLinkChecksSupported(): boolean {
  return checkBridge() !== undefined;
}

/**
 * `workingDir` is part of the identity because a bare relative path means a
 * different file in a different chat. The NUL separator cannot occur in either
 * half — `parseFileLink` rejects every control character — so the two fields
 * can never run together into a colliding key.
 */
function cacheKey(path: string, workingDir?: string): string {
  return `${workingDir ?? ''}\u0000${path}`;
}

const answers = new Map<string, FileLinkExistence>();
const inFlight = new Set<string>();
const queued = new Map<string, FilePathCheckRequest>();
const listeners = new Set<() => void>();
let flushScheduled = false;

function notify(): void {
  for (const listener of [...listeners]) listener();
}

/**
 * Batch a whole message's paths into ONE round trip.
 *
 * Every link asks independently as it mounts, but React commits them in a single
 * task, so their effects all land before the microtask below runs. Ten mentions
 * of one path collapse to one request, and thirty distinct paths to one call.
 */
function flush(): void {
  flushScheduled = false;
  const batch = [...queued.entries()];
  queued.clear();
  if (!batch.length) return;

  const check = checkBridge();
  // The bridge vanished between queueing and flushing (only reachable when a
  // test swaps `window.electron` mid-flight). Leave the entries unanswered
  // rather than recording a verdict this surface is not entitled to give.
  if (!check) return;

  for (const [key] of batch) inFlight.add(key);
  void check(batch.map(([, request]) => request))
    .then((results) => (Array.isArray(results) ? results : []))
    .catch(() => null)
    .then((results) => {
      batch.forEach(([key], index) => {
        inFlight.delete(key);
        // ⚠ A NEGATIVE ANSWER and a FAILED CHECK are different states, and
        // collapsing them costs the wrong thing. `{exists:false}` means we
        // asked and the file is not there — `missing`, plain and inert, which
        // is the whole point of this module. A rejected call, or an answer too
        // short to cover this entry, means we learned NOTHING; that is the same
        // knowledge state as having no bridge at all, so it takes the same
        // verdict `unchecked` and stays clickable. Otherwise one transient IPC
        // hiccup silently de-links every real file in the transcript until the
        // view remounts — turning a rare failure into the exact complaint this
        // module exists to fix, pointed the other way.
        //
        // Still a RECORDED verdict either way: an unanswered key would be
        // re-queued by the next render, forever.
        const answer = results?.[index];
        answers.set(key, answer ? (answer.exists ? 'present' : 'missing') : 'unchecked');
      });
      notify();
    });
}

function requestCheck(path: string, workingDir?: string): void {
  const key = cacheKey(path, workingDir);
  if (answers.has(key) || inFlight.has(key) || queued.has(key)) return;
  queued.set(key, workingDir ? { path, workingDir } : { path });
  if (flushScheduled) return;
  flushScheduled = true;
  queueMicrotask(flush);
}

function subscribe(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  return () => {
    listeners.delete(onStoreChange);
  };
}

/**
 * The existence of `path`, re-rendering the caller once the answer arrives.
 *
 * `path` may be null for a link that is not a file at all (an external URL),
 * which reports `unchecked` and so stays clickable.
 */
export function useFileLinkExistence(
  path: string | null | undefined,
  workingDir?: string
): FileLinkExistence {
  const supported = fileLinkChecksSupported();

  useEffect(() => {
    if (!supported || !path) return;
    requestCheck(path, workingDir);
  }, [supported, path, workingDir]);

  const getSnapshot = useCallback((): FileLinkExistence => {
    if (!path || !fileLinkChecksSupported()) return 'unchecked';
    return answers.get(cacheKey(path, workingDir)) ?? 'checking';
  }, [path, workingDir]);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Drop every cached verdict. Tests only — the cache is process-global. */
export function resetFileLinkStatusForTests(): void {
  answers.clear();
  inFlight.clear();
  queued.clear();
  listeners.clear();
  flushScheduled = false;
}
