import { beforeEach, describe, expect, it, vi } from 'vitest';
import { effectivePrivacy, loadRegistry, REGISTRY_LOAD_BUDGET_MS } from './registry';

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

  it('an upgrade takes effect on the next successful fetch and persists', async () => {
    mockFetch({ extensions: [{ extension_name: 'labarchivesagent', privacy: 'private' }] });
    await loadRegistry();
    mockFetchFailure();
    const { registry, live } = await loadRegistry();
    expect(live).toBe(false);
    expect(effectivePrivacy(registry, 'labarchivesagent')).toBe('private');
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
