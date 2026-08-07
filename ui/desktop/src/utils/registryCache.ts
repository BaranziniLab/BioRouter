/**
 * The last-good BAAM catalogue on disk (issue #56 §10.2).
 *
 * `private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)`. The persisted
 * half of that union lives in the renderer's `localStorage`; this file is what
 * makes the *catalogue* itself survive a restart, so an offline launch shows the
 * entries the machine last saw rather than the snapshot frozen at build time.
 *
 * ⚠ **Deliberately a module of its own, and deliberately Electron-free.**
 * `main.ts` imports `electron` at the top level and cannot be unit-tested, and
 * the renderer's tests stop at the IPC boundary — so with this code inline in
 * `main.ts` an implementation with **no disk cache at all** passed every test
 * this feature has. That is the shape of gap this extraction closes; see
 * `registryCache.test.ts`. Nothing here imports `app`, `log` or `electron`: the
 * caller supplies the path and receives the error to log.
 */

import crypto from 'node:crypto';
import fs from 'node:fs/promises';

export interface CachedRegistry {
  registry: unknown;
  /** ISO-8601, and guaranteed parseable — see `readLastGoodRegistry`. */
  fetchedAt: string;
}

/**
 * A document is only worth caching if it is actually a catalogue.
 *
 * ⚠ **Element-wise, not just array-shaped.** `{"extensions":[null],"skills":[]}`
 * satisfies `Array.isArray` on both members and is nonetheless not a catalogue;
 * admitting it wrote it to the cache, where it was re-admitted on every launch
 * thereafter and threw in the renderer's classifier on a `null.privacy`. The
 * cache is precisely what turns one bad response into a permanent one, so the
 * validation guarding it has to look at what is IN the arrays.
 *
 * Only "is a plain object" is checked. A v1 document's entries carry different
 * fields from a v2 document's, and rejecting on a missing field would refuse
 * catalogues the site legitimately publishes — the point is to exclude a
 * captive portal's login page and outright junk, not to re-declare the schema.
 */
export function isRegistryDocument(json: unknown): boolean {
  if (!json || typeof json !== 'object') return false;
  const doc = json as { extensions?: unknown; skills?: unknown };
  if (!Array.isArray(doc.extensions) || !Array.isArray(doc.skills)) return false;
  const isEntry = (e: unknown) => !!e && typeof e === 'object' && !Array.isArray(e);
  return doc.extensions.every(isEntry) && doc.skills.every(isEntry);
}

/**
 * Write-to-scratch-then-rename, because two writers of this file is the NORMAL
 * case, not a rare one: `ExtensionsSection` calls `loadRegistry()` on mount and
 * it also renders `BrowseExtensionsModal`, which calls it again — two
 * `registry:fetch` handlers in flight at once. A plain `writeFile` truncates and
 * then writes, so two payloads of different lengths can interleave into a file
 * that is neither. `rename` within a directory is atomic, so a reader sees the
 * old document or the new one and never a splice of both.
 *
 * Returns the error rather than throwing or logging it: a cache that cannot be
 * written costs freshness on the next offline run, never the fetch that just
 * succeeded, and the decision about how to report that belongs to the caller
 * that owns the logger.
 */
export async function writeLastGoodRegistry(
  cachePath: string,
  registry: unknown,
  fetchedAt: string
): Promise<Error | null> {
  const scratch = `${cachePath}.${crypto.randomBytes(6).toString('hex')}.tmp`;
  try {
    await fs.writeFile(scratch, JSON.stringify({ fetchedAt, registry }), 'utf8');
    await fs.rename(scratch, cachePath);
    return null;
  } catch (err) {
    await fs.rm(scratch, { force: true }).catch(() => {});
    return err instanceof Error ? err : new Error(String(err));
  }
}

/**
 * The cached catalogue, or `null` for anything that is not one.
 *
 * A cache entry is only usable if it can say HOW OLD it is. The renderer's
 * freshness line reads a missing date as "showing bundled catalog (offline)" —
 * the one thing it can safely conclude, since the bundled snapshot is the only
 * source with no fetch to date. Returning a cached document without one would
 * make that sentence name the wrong catalogue, on the screen where the user
 * decides whether to trust the entries below it. So an undated entry is treated
 * as no cache at all, which falls back to the snapshot the line would have
 * claimed anyway.
 */
export async function readLastGoodRegistry(cachePath: string): Promise<CachedRegistry | null> {
  try {
    const raw = await fs.readFile(cachePath, 'utf8');
    const parsed = JSON.parse(raw) as { registry?: unknown; fetchedAt?: unknown };
    if (!isRegistryDocument(parsed.registry)) return null;
    const fetchedAt = parsed.fetchedAt;
    if (typeof fetchedAt !== 'string' || Number.isNaN(Date.parse(fetchedAt))) return null;
    return { registry: parsed.registry, fetchedAt };
  } catch {
    return null;
  }
}

/** Exactly what `registry:fetch` resolves with; the renderer's `loadRegistry` reads this shape. */
export interface RegistryFetchResult {
  registry?: unknown;
  /** ISO-8601. Present whenever `registry` is. */
  fetchedAt?: string;
  /** `true` when `registry` is the cached copy rather than a fresh fetch. */
  stale?: boolean;
  /** Present only when there is no document to return at all. */
  error?: string;
}

/**
 * Issue #56 §10.2. The fetch had no timeout at all, so a registry host that
 * accepted the connection and then said nothing left the Browse modal on
 * "Loading catalog…" indefinitely — Node's `fetch` has no default timeout.
 *
 * The renderer's own `REGISTRY_LOAD_BUDGET_MS` (11 s) is deliberately just
 * ABOVE this, so a healthy main process always answers first and the renderer's
 * budget only fires when the IPC channel itself is stuck.
 */
export const REGISTRY_FETCH_TIMEOUT_MS = 10_000;

/**
 * Fetch the catalogue, cache what succeeded, replay what last did.
 *
 * ⚠ **The whole handler lives here, not just its primitives.** Review found
 * that `main.ts`'s `registry:fetch` composed these three functions and that
 * nothing tested the composition — *"an implementation that imported
 * `registryCache` and never called it would pass every test in this feature"* —
 * and that the 10 s timeout was checked by `grep -c AbortController`, which
 * passes on the word in a comment and on a controller whose `signal` never
 * reaches `fetch`. `main.ts` imports Electron at the top level and cannot be
 * unit-tested, so the fix is the one that already moved the cache out of it:
 * move the composition too, and leave behind only what genuinely needs `app`
 * (the path) and `log` (the warning).
 *
 * `fetchImpl`, `now` and the injected timeout exist for those tests and for
 * nothing else; all three default to the real thing.
 *
 * Three orderings are load-bearing and each is asserted:
 *   - validate BEFORE caching — a captive portal's login page is a 200 with a
 *     body, and caching it poisons every subsequent offline launch;
 *   - a write failure costs freshness later, never the fetch that just
 *     succeeded, so it is reported and swallowed;
 *   - any failure — network, non-2xx, unparseable, or not-a-catalogue — replays
 *     the cache as `stale: true` rather than surfacing an error, because the
 *     catalogue the machine last saw beats no catalogue at all.
 */
export async function fetchRegistryWithLastGood(options: {
  url: string;
  cachePath: string;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
  now?: () => Date;
  onWriteError?: (err: Error) => void;
}): Promise<RegistryFetchResult> {
  const {
    url,
    cachePath,
    timeoutMs = REGISTRY_FETCH_TIMEOUT_MS,
    fetchImpl = fetch,
    now = () => new Date(),
    onWriteError,
  } = options;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetchImpl(url, {
      headers: { 'User-Agent': 'Biorouter', Accept: 'application/json' },
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const json: unknown = await response.json();
    // Validate BEFORE caching: a proxy's login page is a 200 with a body, and
    // caching it would poison every subsequent offline launch.
    if (!isRegistryDocument(json)) throw new Error('Response was not a marketplace catalog');
    const fetchedAt = now().toISOString();
    const writeError = await writeLastGoodRegistry(cachePath, json, fetchedAt);
    if (writeError) onWriteError?.(writeError);
    return { registry: json, fetchedAt, stale: false };
  } catch (err) {
    const cached = await readLastGoodRegistry(cachePath);
    if (cached) {
      return { registry: cached.registry, fetchedAt: cached.fetchedAt, stale: true };
    }
    return { error: (err as Error).message };
  } finally {
    clearTimeout(timer);
  }
}
