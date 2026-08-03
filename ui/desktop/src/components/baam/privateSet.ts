/**
 * The persisted half of `private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)`
 * (issue #56, §10.2).
 *
 * **Freshness raises, never lowers.** An upgrade — an extension newly marked
 * private upstream — takes effect on the next successful fetch and then persists
 * across restarts, offline runs and a wiped last-good document. A downgrade is
 * simply never applied: nothing in this module removes a key. An offline laptop
 * can fail to *learn* a new private badge; it can never *lose* one.
 *
 * That asymmetry is the whole point, and it is why this is a separate store
 * rather than a field read back off the cached registry document. A cached
 * document is replaced wholesale by the next successful fetch, so a compromised
 * or merely reverted `registry.json` would take the badge with it. This set only
 * ever grows.
 *
 * Deliberately dependency-free: it stores **already-reduced keys** and does no
 * reduction of its own, so there is exactly one implementation of the
 * name→key rule in the renderer (`nameToKey`) and this module cannot drift from
 * it. Callers reduce; this stores.
 *
 * It is a *mirror*, not enforcement. Nothing here grants or revokes access —
 * Gates C/E/F key on the Rust `PRIVATE_EXTENSIONS` baseline, which no renderer
 * can widen. This exists only so the GUI can say what the daemon will do.
 */

const STORAGE_KEY = 'biorouter.baam.privateExtensionKeys';

/**
 * Parsing localStorage on every call would be wasteful (`classifyExtension` runs
 * once per extension per render), and caching it in a plain module variable
 * would survive a test's `localStorage.clear()` and let one test's learned key
 * leak into the next. Keying the cache on the raw string gets both: one parse
 * per distinct value, and a clear is observed immediately.
 */
let cachedRaw: string | null | undefined;
let cachedSet: ReadonlySet<string> = new Set();

function rawValue(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    // Storage can be unavailable (disabled, quota, a non-browser context). A
    // missing learned set is a fail-to-learn, which is the safe direction.
    return null;
  }
}

/** The keys learned from every successful fetch so far. Never shrinks. */
export function learnedPrivateExtensionKeys(): ReadonlySet<string> {
  const raw = rawValue();
  if (raw === cachedRaw) return cachedSet;
  cachedRaw = raw;
  cachedSet = parse(raw);
  return cachedSet;
}

function parse(raw: string | null): ReadonlySet<string> {
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((k): k is string => typeof k === 'string' && k.length > 0));
  } catch {
    return new Set();
  }
}

/**
 * Union `keys` into the persisted set. Adding only: passing a smaller set than
 * last time removes nothing, which is the invariant this module exists for.
 */
export function rememberPrivateExtensionKeys(keys: Iterable<string>): void {
  const next = new Set(learnedPrivateExtensionKeys());
  let grew = false;
  for (const key of keys) {
    if (key && !next.has(key)) {
      next.add(key);
      grew = true;
    }
  }
  if (!grew) return;
  try {
    const raw = JSON.stringify([...next].sort());
    localStorage.setItem(STORAGE_KEY, raw);
    cachedRaw = raw;
    cachedSet = next;
  } catch {
    // Unwritable storage means the badge is re-learned on the next fetch rather
    // than remembered. Still the safe direction; never a reason to fail a load.
  }
}
