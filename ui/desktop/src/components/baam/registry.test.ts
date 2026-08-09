import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  catalogFreshnessLine,
  effectivePrivacy,
  extensionProvenance,
  loadRegistry,
  REGISTRY_LOAD_BUDGET_MS,
  type BaamRegistry,
} from './registry';

/**
 * Issue #56, §10.2 — `private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)`.
 *
 * These three tests are the ONLY thing standing between a compromised or merely
 * stale `registry.json` and `ucsfomopagent` losing its private badge on every
 * machine that fetches it, so run them by FILE PATH. As a `-t` name filter the
 * bare word `registry` matches three unrelated suites (`terminalSessionRegistry`,
 * `newTabRegistry`, `closeActiveTabRegistry`), vitest finds files, the filter
 * then skips all of their tests, and the run exits 0 having executed none of
 * these.
 */

const fetchRegistry = vi.fn();

/** One live document, with whatever a test cares about laid over a valid shell. */
function mockFetch(doc: { extensions?: unknown[]; skills?: unknown[] }) {
  fetchRegistry.mockResolvedValue({
    registry: { version: 2, source: 'test', skills: [], extensions: [], ...doc },
    fetchedAt: new Date().toISOString(),
    stale: false,
  });
}

function mockFetchFailure() {
  fetchRegistry.mockResolvedValue({ error: 'offline' });
}

/** The main process never answers — a wedged handler, not a rejected fetch. */
function mockFetchHang() {
  fetchRegistry.mockImplementation(() => new Promise(() => {}));
}

beforeEach(() => {
  fetchRegistry.mockReset();
  // Clears the persisted last-good private set between tests, so test order
  // cannot manufacture a pass.
  localStorage.clear();
  (window as unknown as { electron: unknown }).electron = { fetchRegistry };
});

describe('loadRegistry — freshness that raises and never lowers', () => {
  it('a downgrade is never honoured for an entry the compiled set names', async () => {
    // private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch). An offline
    // laptop can fail to LEARN a new private badge; it can never LOSE one.
    mockFetch({ extensions: [{ extension_name: 'ucsfomopagent', privacy: 'public' }] });
    const { registry } = await loadRegistry();
    expect(effectivePrivacy(registry, 'ucsfomopagent')).toBe('private');
  });

  /**
   * ⚠ **"Persists" is asserted against storage and against a restarted module
   * graph, not against a second call in the same one.**
   *
   * The two `loadRegistry` calls this test used to make shared one module
   * instance, so a learned set held only in a module variable was
   * indistinguishable from a durable one: measured, replacing `privateSet.ts`'s
   * localStorage read AND write with an in-memory `Set` — learning lost on every
   * window reload and every app restart — left all eight tests in this file
   * green.
   *
   * `localStorage['biorouter.baam.privateExtensionKeys']` is the *only* durable
   * carrier of a **learned** badge. The last-good document on disk
   * (`utils/registryCache.ts`) is a different mechanism and cannot stand in for
   * it: a cached document that no longer marks the key private would not
   * re-teach it. So both halves are asserted here — the write reached storage,
   * and a fresh module graph reads it back.
   *
   * `labarchivesagent` is deliberately NOT in the compiled baseline
   * (`registry.fallback.json` publishes it as `privacy: public`), so nothing
   * below can pass off the compiled half of the union.
   */
  it('an upgrade takes effect on the next successful fetch and persists', async () => {
    mockFetch({ extensions: [{ extension_name: 'labarchivesagent', privacy: 'private' }] });
    await loadRegistry();

    // The durable carrier itself, read directly. A module-variable-only store
    // never gets here.
    const stored: unknown = JSON.parse(
      localStorage.getItem('biorouter.baam.privateExtensionKeys') ?? 'null'
    );
    expect(stored).toContain('labarchivesagent');

    // …and a restarted window — a fresh module graph, so fresh module-level
    // caches and a fresh in-memory `unpersisted` set — reads it back off
    // storage with no successful fetch of its own to learn from.
    vi.resetModules();
    const restarted = await import('./registry');
    mockFetchFailure();
    const { registry, live } = await restarted.loadRegistry();
    expect(live).toBe(false);
    expect(restarted.effectivePrivacy(registry, 'labarchivesagent')).toBe('private');
  });

  /**
   * "Never lowers" has to hold when our own bookkeeping fails, not only when it
   * works. `localStorage.setItem` throws on a quota, in private-browsing modes,
   * and wherever storage is disabled — and the learned key was then neither
   * persisted NOR retained in memory, so the very next document could take the
   * badge back off. A failure to write is a reason to re-learn later; it is
   * never a reason to forget now.
   *
   * ⚠ The key is unique to this test on purpose: an unpersisted key is held for
   * the life of the module and, having never reached storage, is not removed by
   * another test's `localStorage.clear()`.
   *
   * ⚠ The spy goes on `localStorage` itself, NOT on `Storage.prototype`. This
   * suite's setup (`src/test/setup.ts`) installs its own `MemoryStorage` class
   * because jsdom's storage is unusable here, so a `Storage.prototype` spy
   * intercepts nothing, `setItem` succeeds, and the test passes against the very
   * bug it is written to catch — measured, not theorised.
   */
  it('a learned badge survives a storage write that fails', async () => {
    const setItem = vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError');
    });
    try {
      mockFetch({ extensions: [{ extension_name: 'unwritablequotaagent', privacy: 'private' }] });
      await loadRegistry();
      mockFetch({ extensions: [{ extension_name: 'unwritablequotaagent', privacy: 'public' }] });
      const { registry } = await loadRegistry();
      expect(effectivePrivacy(registry, 'unwritablequotaagent')).toBe('private');
    } finally {
      setItem.mockRestore();
    }
  });

  /**
   * The threat model this task names is "a compromised or merely stale
   * `registry.json`", and the last-good cache is what turns a one-shot bad
   * response into a permanent one. `{"extensions":[null],"skills":[]}` is a 200
   * whose `extensions` IS an array, so an array-shaped check admits it, caches
   * it, and re-admits it on every launch thereafter.
   *
   * The failure was not a wrong badge — it was `loadRegistry` REJECTING, from
   * `rememberPrivateExtensions` outside the `try` whose comment promises it
   * never does. Browse fell to its error state instead of the bundled snapshot,
   * `ExtensionsSection` took an unhandled rejection and dropped every §13.5
   * line, and `effectivePrivacy` threw inside React render.
   */
  it('a document with a malformed entry loads, and still badges the compiled set', async () => {
    mockFetch({ extensions: [null, 'nonsense', { extension_name: 'ucsfomopagent' }] });
    const { registry } = await loadRegistry();
    expect(effectivePrivacy(registry, 'ucsfomopagent')).toBe('private');
    expect(effectivePrivacy(registry, 'spokeagent')).toBe('public');
  });

  /**
   * The OTHER layer, pinned by its own contract rather than by the classifier's.
   *
   * `withUsableEntriesOnly` is what lets every downstream consumer assume an
   * entry is an object — the two Browse lists map over these arrays and read
   * `.name`, `.description` and `.tags` off each one, so a single `null` is a
   * `TypeError` inside React render, in files with no test of this input. The
   * classifier survives without this layer (it filters again internally, on
   * purpose), which is exactly why the boundary needs an assertion of its own:
   * review measured that removing it left every other test here green.
   */
  it('strips non-entries before any consumer sees the document', async () => {
    mockFetch({
      extensions: [null, 'nonsense', [], { extension_name: 'ucsfomopagent' }],
      skills: [null, { id: 'r-scripting' }],
    });
    const { registry } = await loadRegistry();
    expect(registry.extensions).toEqual([{ extension_name: 'ucsfomopagent' }]);
    expect(registry.skills).toEqual([{ id: 'r-scripting' }]);
  });

  /**
   * The same hostile document, but handed STRAIGHT to the classifier.
   *
   * Two layers drop non-object entries: `withUsableEntriesOnly` at
   * `loadRegistry`'s boundary, and the `isEntry` filter inside `privateKeysIn`.
   * They are deliberately redundant — but review measured that removing *either*
   * one left every test in this file green, because the test above reaches the
   * classifier only through the boundary, so the boundary alone satisfies it.
   *
   * This pins the inner layer, and it is not a hypothetical: `effectivePrivacy`
   * is documented as callable with a document that never went through
   * `loadRegistry`, it runs during React render, and a classifier that throws on
   * a hostile input has failed in the one direction it exists to prevent.
   */
  it('classifies a document that never went through the boundary', () => {
    const raw = {
      version: 2,
      source: 'test',
      skills: [],
      extensions: [null, 'nonsense', [], { extension_name: 'cdwagent', privacy: 'private' }],
    } as unknown as BaamRegistry;

    expect(() => effectivePrivacy(raw, 'ucsfomopagent')).not.toThrow();
    expect(effectivePrivacy(raw, 'ucsfomopagent')).toBe('private');
    expect(effectivePrivacy(raw, 'cdwagent')).toBe('private');
    expect(effectivePrivacy(raw, 'spokeagent')).toBe('public');
    // …and the prose that travels with the badge is total on the same input.
    expect(() => extensionProvenance(raw, 'spokeagent')).not.toThrow();
  });

  it('a hung registry does not hang the modal', async () => {
    // Fake timers, not an eleven-second wall-clock wait: the assertion the plan
    // wrote is `< 12_000`, and the renderer's budget is deliberately just above
    // the main process's 10 s AbortController so a healthy main answers first.
    // If `loadRegistry` had no budget of its own this never settles and the test
    // fails on vitest's own timeout, which is exactly the discrimination wanted.
    vi.useFakeTimers();
    try {
      mockFetchHang();
      const t0 = Date.now();
      const pending = loadRegistry();
      await vi.advanceTimersByTimeAsync(REGISTRY_LOAD_BUDGET_MS + 1);
      const { live } = await pending;
      expect(live).toBe(false);
      expect(Date.now() - t0).toBeLessThan(12_000);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('the prose the badge travels with', () => {
  const empty: BaamRegistry = { version: 2, source: 'test', extensions: [], skills: [] };

  /**
   * §13.5's private string asserts *publication*, and the union has a branch
   * where that is simply not true: a key learned from a catalogue that no longer
   * lists it, or the compiled baseline read against a document that does not.
   * The old comment claimed "every branch of which is a marketplace source — so
   * naming the marketplace here is always true", which this is the
   * counterexample to.
   */
  it('does not claim publication for a name the catalogue in hand does not list', () => {
    const line = extensionProvenance(empty, 'cdwagent');
    expect(line).toMatch(/^Private/);
    expect(line).not.toMatch(/published on the Biorouter marketplace/);
  });

  it('still uses §13.5 verbatim when the catalogue does list it', () => {
    const listed: BaamRegistry = {
      ...empty,
      extensions: [{ extension_name: 'cdwagent', privacy: 'private' } as never],
    };
    expect(extensionProvenance(listed, 'cdwagent')).toBe(
      'Private: published on the Biorouter marketplace'
    );
  });

  /**
   * "Showing bundled catalog (offline)" names WHICH catalogue is on screen. Said
   * over a cached one it is a false statement about the thing the user is
   * looking at, and it is the sentence they would use to decide whether to
   * trust the entries.
   */
  it('never calls a catalogue bundled unless it is the bundled one', () => {
    expect(catalogFreshnessLine({ live: false })).toMatch(/bundled/i);
    expect(catalogFreshnessLine({ live: false, fetchedAt: '2026-01-02T03:04:05Z' })).not.toMatch(
      /bundled/i
    );
    expect(catalogFreshnessLine({ live: false, fetchedAt: 'not-a-date' })).not.toMatch(/bundled/i);
    expect(catalogFreshnessLine({ live: true })).toBeNull();
  });
});
