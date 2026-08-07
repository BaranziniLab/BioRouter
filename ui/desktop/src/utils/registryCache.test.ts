import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import {
  fetchRegistryWithLastGood,
  isRegistryDocument,
  readLastGoodRegistry,
  REGISTRY_FETCH_TIMEOUT_MS,
  writeLastGoodRegistry,
} from './registryCache';

/**
 * Issue #56 §10.2 — the last-good half of `private_set = PRIVATE_EXTENSIONS ∪
 * private(last_good_fetch)`.
 *
 * ⚠ **This file exists because the renderer tests cannot reach here.**
 * `registry.test.ts` mocks `window.electron.fetchRegistry`, which is the IPC
 * boundary — so an implementation with **no disk cache at all** passes every one
 * of those tests, and the persistence half of the rule ("an offline laptop can
 * fail to learn a badge, never lose one" *across restarts*) would be unprotected.
 * That is why these three functions live in a module of their own rather than
 * inside `main.ts`: `main.ts` imports Electron at the top level and cannot be
 * unit-tested, and code that cannot be tested is where this class of bug lives.
 */

let dir: string;
let cachePath: string;

const catalogue = {
  version: 2,
  source: 'test',
  extensions: [{ extension_name: 'ucsfomopagent', privacy: 'private' }],
  skills: [{ id: 'r-scripting' }],
};

beforeEach(async () => {
  dir = await fs.mkdtemp(path.join(os.tmpdir(), 'biorouter-registry-cache-'));
  cachePath = path.join(dir, 'registry-last-good.json');
});

afterEach(async () => {
  await fs.rm(dir, { recursive: true, force: true });
});

describe('isRegistryDocument', () => {
  it('accepts a catalogue', () => {
    expect(isRegistryDocument(catalogue)).toBe(true);
    expect(isRegistryDocument({ extensions: [], skills: [] })).toBe(true);
  });

  /**
   * The whole point of validating BEFORE caching. A captive portal's login page
   * is a 200 with a body, and `{"extensions":[null],"skills":[]}` satisfies
   * `Array.isArray` on both members while not being a catalogue at all —
   * admitting it wrote it to disk, where it was re-admitted on every launch
   * thereafter and threw in the renderer's classifier.
   */
  it('rejects a document whose entries are not entries', () => {
    expect(isRegistryDocument({ extensions: [null], skills: [] })).toBe(false);
    expect(isRegistryDocument({ extensions: [], skills: ['nonsense'] })).toBe(false);
    expect(isRegistryDocument({ extensions: [[]], skills: [] })).toBe(false);
  });

  it('rejects anything that is not a catalogue at all', () => {
    expect(isRegistryDocument(null)).toBe(false);
    expect(isRegistryDocument('<html>login</html>')).toBe(false);
    expect(isRegistryDocument({ extensions: [] })).toBe(false);
  });
});

describe('the last-good cache on disk', () => {
  /**
   * The test the IPC mock cannot write. Without a real round trip through a real
   * file, "there is no cache" and "there is a cache" are indistinguishable to
   * every other test in this feature.
   */
  it('round-trips a catalogue and its date', async () => {
    expect(
      await writeLastGoodRegistry(cachePath, catalogue, '2026-01-02T03:04:05.000Z')
    ).toBeNull();

    const cached = await readLastGoodRegistry(cachePath);
    expect(cached).not.toBeNull();
    expect(cached!.fetchedAt).toBe('2026-01-02T03:04:05.000Z');
    expect(cached!.registry).toEqual(catalogue);
  });

  it('reports no cache rather than throwing when there is no file', async () => {
    expect(await readLastGoodRegistry(cachePath)).toBeNull();
  });

  /**
   * A cache entry that cannot say how old it is would make the renderer's
   * freshness line say "showing bundled catalog (offline)" over a cached one —
   * naming the wrong catalogue on the screen where the user decides whether to
   * trust the entries under it. Treating it as no cache falls back to the
   * snapshot the line would have claimed anyway.
   */
  it('treats an undated or misdated entry as no cache', async () => {
    await fs.writeFile(cachePath, JSON.stringify({ registry: catalogue }), 'utf8');
    expect(await readLastGoodRegistry(cachePath)).toBeNull();

    await fs.writeFile(
      cachePath,
      JSON.stringify({ registry: catalogue, fetchedAt: 'whenever' }),
      'utf8'
    );
    expect(await readLastGoodRegistry(cachePath)).toBeNull();
  });

  it('treats a poisoned or unparseable cached document as no cache', async () => {
    await fs.writeFile(
      cachePath,
      JSON.stringify({ registry: { extensions: [null], skills: [] }, fetchedAt: '2026-01-02' }),
      'utf8'
    );
    expect(await readLastGoodRegistry(cachePath)).toBeNull();

    await fs.writeFile(cachePath, 'not json at all', 'utf8');
    expect(await readLastGoodRegistry(cachePath)).toBeNull();
  });

  /**
   * Two writers of this file is the NORMAL case — `ExtensionsSection` loads the
   * registry on mount and also renders `BrowseExtensionsModal`, which loads it
   * again — so the live file is never truncated: the payload goes to a scratch
   * name and is `rename`d over the target, which is atomic within a directory.
   *
   * ⚠ Asserted on the CALL, not on the outcome. "The directory ends up holding
   * one good file" is true of a plain `writeFile` too, and so is "no scratch
   * file is left behind" — a torn read is a race, and a test that has to lose
   * that race to fail is a test that passes for the wrong reason most of the
   * time. What distinguishes the two implementations deterministically is that
   * the atomic one never names the target as a write destination at all.
   */
  it('never writes the live file directly', async () => {
    const writeFile = vi.spyOn(fs, 'writeFile');
    try {
      await writeLastGoodRegistry(cachePath, catalogue, '2026-01-02T03:04:05.000Z');
      expect(writeFile).toHaveBeenCalled();
      for (const call of writeFile.mock.calls) {
        expect(call[0]).not.toBe(cachePath);
      }
    } finally {
      writeFile.mockRestore();
    }
    // …and the scratch name does not survive. A leaked temp file per fetch would
    // quietly fill the user data directory.
    expect(await fs.readdir(dir)).toEqual(['registry-last-good.json']);
  });

  it('returns the error instead of throwing when the path is unwritable', async () => {
    const unwritable = path.join(dir, 'no', 'such', 'directory', 'cache.json');
    expect(await writeLastGoodRegistry(unwritable, catalogue, '2026-01-02')).toBeInstanceOf(Error);
  });
});

/**
 * ⚠ **The composition, not just its primitives.**
 *
 * Review found that these three functions were composed inside `main.ts`'s
 * `registry:fetch` handler and that nothing tested the composition: *"an
 * implementation that imported `registryCache` and never called it would pass
 * every test in this feature."* The `fetch fails → replay the cache with
 * `stale: true`` branch — the one the whole last-good design exists for — was
 * covered nowhere, and the 10 s timeout was gated by `grep -c AbortController`,
 * which passes on the word in a comment and on a controller whose `signal` never
 * reaches `fetch`.
 *
 * Moving the handler body here is what makes all of that assertable, for the
 * same reason the cache itself was moved: `main.ts` imports Electron at the top
 * level and cannot be unit-tested.
 */
describe('fetchRegistryWithLastGood — the handler, end to end', () => {
  const url = 'https://biorouter.ucsf.edu/registry.json';

  /** A `fetch` that answers with `body`, exactly as the real one would. */
  function respond(body: unknown, ok = true, status = 200): typeof fetch {
    return (async () => ({ ok, status, json: async () => body })) as unknown as typeof fetch;
  }

  it('returns a live document and writes it to the cache', async () => {
    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      fetchImpl: respond(catalogue),
      now: () => new Date('2026-01-02T03:04:05.000Z'),
    });

    expect(result).toEqual({
      registry: catalogue,
      fetchedAt: '2026-01-02T03:04:05.000Z',
      stale: false,
    });
    // …and it is on disk, so the next offline launch has something to replay.
    expect(await readLastGoodRegistry(cachePath)).toEqual({
      registry: catalogue,
      fetchedAt: '2026-01-02T03:04:05.000Z',
    });
  });

  /**
   * The branch the last-good cache exists for, and the one that was untested.
   * `stale: true` is what makes the renderer report `live: false` and print
   * "catalogue last updated …" instead of claiming the bundled snapshot.
   */
  it('replays the cached document as stale when the fetch fails', async () => {
    await writeLastGoodRegistry(cachePath, catalogue, '2026-01-02T03:04:05.000Z');

    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      fetchImpl: (() => Promise.reject(new Error('offline'))) as unknown as typeof fetch,
    });

    expect(result).toEqual({
      registry: catalogue,
      fetchedAt: '2026-01-02T03:04:05.000Z',
      stale: true,
    });
  });

  it('reports the error only when there is no cache to fall back to', async () => {
    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      fetchImpl: (() => Promise.reject(new Error('offline'))) as unknown as typeof fetch,
    });
    expect(result).toEqual({ error: 'offline' });
  });

  /**
   * A captive portal's login page is a 200 with a body. Validating AFTER caching
   * would write it, and `readLastGoodRegistry` would then hand it back on every
   * launch thereafter — one bad response made permanent, which is precisely what
   * a cache is capable of turning a transient failure into.
   */
  it('neither returns nor caches a 200 that is not a catalogue', async () => {
    await writeLastGoodRegistry(cachePath, catalogue, '2026-01-02T03:04:05.000Z');

    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      fetchImpl: respond('<html>sign in to the wifi</html>'),
    });

    // The good cache is replayed, and is still the good cache afterwards.
    expect(result.stale).toBe(true);
    expect(result.registry).toEqual(catalogue);
    expect((await readLastGoodRegistry(cachePath))!.registry).toEqual(catalogue);
  });

  it('treats a non-2xx as a failure rather than parsing the error page', async () => {
    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      fetchImpl: respond({ nope: true }, false, 503),
    });
    expect(result).toEqual({ error: 'HTTP 503' });
  });

  /**
   * ⚠ **The timeout, asserted instead of grepped.**
   *
   * `fetchImpl` here settles ONLY when it is handed an abort signal that fires.
   * So this discriminates the three implementations `grep -c AbortController`
   * cannot tell apart: no timeout at all, a controller that is constructed and
   * never wired into `fetch`, and the word appearing in a comment. Each of those
   * leaves this promise pending forever and fails on vitest's own timeout.
   *
   * 25 ms rather than the real 10 s because the interval under test is injected;
   * the production default is asserted separately below, since a test that
   * passes its own timeout in cannot speak for the shipped one.
   */
  it('aborts a host that accepts the connection and then says nothing', async () => {
    await writeLastGoodRegistry(cachePath, catalogue, '2026-01-02T03:04:05.000Z');

    const hangs = ((_url: string, init?: { signal?: AbortSignal }) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener('abort', () => reject(new Error('aborted')));
      })) as unknown as typeof fetch;

    const result = await fetchRegistryWithLastGood({
      url,
      cachePath,
      timeoutMs: 25,
      fetchImpl: hangs,
    });

    expect(result.stale).toBe(true);
    expect(result.registry).toEqual(catalogue);
  });

  it('ships a ten-second budget, below the renderer’s eleven', () => {
    expect(REGISTRY_FETCH_TIMEOUT_MS).toBe(10_000);
  });

  /**
   * A cache that cannot be written costs freshness on the NEXT offline run and
   * must never cost the fetch that just succeeded — the document is in hand and
   * returning an error over a bookkeeping failure would be strictly worse.
   */
  it('still returns the live document when the cache cannot be written', async () => {
    const onWriteError = vi.fn();
    const result = await fetchRegistryWithLastGood({
      url,
      cachePath: path.join(dir, 'no', 'such', 'directory', 'cache.json'),
      fetchImpl: respond(catalogue),
      now: () => new Date('2026-01-02T03:04:05.000Z'),
      onWriteError,
    });

    expect(result.stale).toBe(false);
    expect(result.registry).toEqual(catalogue);
    expect(onWriteError).toHaveBeenCalledOnce();
  });
});
