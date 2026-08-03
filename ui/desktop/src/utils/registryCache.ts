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
