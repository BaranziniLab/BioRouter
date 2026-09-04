import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from 'react';

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

/** The batched answer: a lookup over the requested set, plus its settledness. */
export type FileLinkExistences = {
  /** The verdict for one requested path; `unchecked` for anything not asked. */
  of: (path: string, workingDir?: string) => FileLinkExistence;
  /** True while at least one requested path is still `checking`. */
  pending: boolean;
};

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

/**
 * ⚠ A `missing` verdict is PROVISIONAL; a `present` one is not.
 *
 * The first version of this cache was permanent in both directions, and that
 * was wrong in the commonest case there is: the assistant *names* a path while
 * streaming prose, and only then calls the tool that creates it. The link mounts
 * during the prose, banks `missing`, and — because `requestCheck` returns early
 * for any key already in the map — never asks again. The file appears a second
 * later and the link stays grey and inert for the life of the renderer.
 *
 * The asymmetry is the fix, and it follows from what the two answers mean. A
 * file that exists essentially never stops existing mid-session, so `present`
 * can be trusted indefinitely. A file that is missing very often appears moments
 * later, so `missing` earns only a short life. Re-asking is cheap: one batched
 * IPC call returning two booleans.
 */
type Answer = { verdict: FileLinkExistence; at: number };

/** How long a `missing` verdict is believed before it is re-checked. */
const MISSING_TTL_MS = 2000;

const answers = new Map<string, Answer>();
const inFlight = new Set<string>();
const queued = new Map<string, FilePathCheckRequest>();
const listeners = new Set<() => void>();
let flushScheduled = false;

/**
 * Monotonic store version, for the batched hook below.
 *
 * A per-path `getSnapshot` can return the verdict itself, because a verdict is a
 * string and React can compare it. A hook watching a WHOLE SET has no such
 * value — building a Map per render would be a fresh object every time and spin
 * `useSyncExternalStore` forever — so it subscribes to this counter instead and
 * derives the answers from `answers` under a `useMemo` keyed on it.
 */
let revision = 0;

function notify(): void {
  revision += 1;
  for (const listener of [...listeners]) listener();
}

function getRevision(): number {
  return revision;
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
        const verdict: FileLinkExistence = answer
          ? answer.exists
            ? 'present'
            : 'missing'
          : 'unchecked';
        answers.set(key, { verdict, at: Date.now() });
      });
      notify();
    });
}

/** Is the recorded answer still believable? */
function isFresh(answer: Answer | undefined): boolean {
  if (!answer) return false;
  // Only `missing` expires. `unchecked` means the check itself failed, and
  // re-asking on a timer would spin against a broken bridge; a remount re-asks.
  //
  // ⚠ This clause is the SECONDARY guard, not the primary one — mutation
  // testing showed that deleting it breaks nothing, because the re-ask timer in
  // the hook below fires only for `missing`, so an expired `present` is never
  // actually re-asked. What this line buys is that a remount, or a sweep
  // triggered by some other link, does not re-ask about a file already found.
  // Keep it, but do not mistake it for what stops the polling.
  if (answer.verdict !== 'missing') return true;
  return Date.now() - answer.at < MISSING_TTL_MS;
}

function requestCheck(path: string, workingDir?: string): void {
  const key = cacheKey(path, workingDir);
  if (isFresh(answers.get(key)) || inFlight.has(key) || queued.has(key)) return;
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
    return answers.get(cacheKey(path, workingDir))?.verdict ?? 'checking';
  }, [path, workingDir]);

  const existence = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  // ⚠ Expiring the verdict is not enough on its own — something has to ASK
  // again. This effect's own dependencies (`path`, `workingDir`) never change
  // for a mounted link, so without this timer a `missing` answer would expire
  // in the map and still be the last word on screen: the re-ask would happen
  // only if some other link happened to mount later. That is the difference
  // between a file that becomes clickable a moment after the agent writes it
  // and one that stays grey until the view is remounted.
  //
  // Only `missing` schedules a wake-up. `present` is final, `checking` already
  // has a request in flight, and `unchecked` means the bridge is absent or the
  // call failed — retrying that on a timer would spin.
  useEffect(() => {
    if (!supported || !path || existence !== 'missing') return;
    const timer = setTimeout(() => requestCheck(path, workingDir), MISSING_TTL_MS);
    return () => clearTimeout(timer);
  }, [supported, path, workingDir, existence]);

  return existence;
}

/**
 * The verdicts for a WHOLE SET of paths, in one subscription and one batch.
 *
 * {@link useFileLinkExistence} is per-link because a link IS a component; the
 * artifact panel holds its paths as data — one list, rebuilt whenever the
 * transcript changes — and cannot mount a hook per row. Same cache, same
 * batching, same four states; the only additions are that the answer is a
 * lookup rather than a value, and that a list can be *partly* answered, which
 * the caller has to be able to see.
 *
 * `pending` is that visibility, and it exists for one specific caller need:
 * something that takes a ONE-TIME baseline of the artifact list (the panel's
 * auto-open snapshot) must not take it mid-sweep, or every path confirmed a
 * moment later reads as newly created. It is false wherever no answer is
 * coming — no bridge, or an empty request list — so it can never wedge.
 *
 * @param requests the paths to ask about. Re-derived arrays are fine: identity
 *   is taken from the CONTENT, so an equal list never re-asks.
 */
export function useFileLinkExistences(
  requests: readonly FilePathCheckRequest[]
): FileLinkExistences {
  const supported = fileLinkChecksSupported();
  const signature = useMemo(
    () => requests.map((request) => cacheKey(request.path, request.workingDir)).join('\u0001'),
    [requests]
  );
  // Everything below is keyed on the signature rather than the array, so the
  // array is read through a ref: closing over it would pin the first list a
  // given signature was seen with.
  const requestsRef = useRef(requests);
  requestsRef.current = requests;

  useEffect(() => {
    if (!supported) return;
    for (const request of requestsRef.current) requestCheck(request.path, request.workingDir);
  }, [supported, signature]);

  const storeRevision = useSyncExternalStore(subscribe, getRevision, getRevision);

  const verdicts = useMemo(() => {
    const map = new Map<string, FileLinkExistence>();
    for (const request of requestsRef.current) {
      const key = cacheKey(request.path, request.workingDir);
      map.set(key, supported ? (answers.get(key)?.verdict ?? 'checking') : 'unchecked');
    }
    return map;
    // `storeRevision` is the subscription: it is what makes this recompute when
    // an answer lands, and is deliberately not read in the body.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [supported, signature, storeRevision]);

  // A `missing` verdict is provisional here for the same reason it is for a
  // single link — the assistant routinely names a file just before writing it —
  // so ask once more when the TTL expires. One timer for the whole set; the
  // re-ask re-batches, and `requestCheck` skips whatever is still fresh.
  //
  // Deliberately ONE retry per distinct missing set, exactly as the single-path
  // hook behaves: re-asking a path that is still gone leaves the verdict string
  // unchanged, so this effect does not re-fire and nothing polls. What actually
  // rescues a file written later is the tool call that wrote it — which reaches
  // the panel as its own confirmed artifact.
  const missingSignature = useMemo(
    () =>
      [...verdicts]
        .filter(([, verdict]) => verdict === 'missing')
        .map(([key]) => key)
        .join('\u0001'),
    [verdicts]
  );
  useEffect(() => {
    if (!supported || !missingSignature) return;
    const timer = setTimeout(() => {
      for (const request of requestsRef.current) requestCheck(request.path, request.workingDir);
    }, MISSING_TTL_MS);
    return () => clearTimeout(timer);
  }, [supported, missingSignature]);

  return useMemo(
    () => ({
      of: (path: string, workingDir?: string): FileLinkExistence =>
        // A path outside the requested set was never asked about, which is the
        // absence of an answer, not a negative one.
        verdicts.get(cacheKey(path, workingDir)) ?? 'unchecked',
      pending: [...verdicts.values()].some((verdict) => verdict === 'checking'),
    }),
    [verdicts]
  );
}

/** Drop every cached verdict. Tests only — the cache is process-global. */
export function resetFileLinkStatusForTests(): void {
  answers.clear();
  inFlight.clear();
  queued.clear();
  listeners.clear();
  flushScheduled = false;
  // Monotonic, never reset: a mounted `useSyncExternalStore` compares the
  // counter it last read, so rewinding it would read as "nothing changed".
  revision += 1;
}
