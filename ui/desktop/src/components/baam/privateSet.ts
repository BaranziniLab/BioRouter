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
 *
 * ## What "only ever grows" costs, stated rather than left implicit
 *
 * This is persistent local state that a remote document writes to. One
 * successful fetch of a `registry.json` marking arbitrary extensions private
 * adds those keys here permanently, and `classifyExtension` consults it — so
 * `extensionPairingRefused` will mark them "unavailable in new chats" on every
 * public-model chat, and `BottomMenuExtensionSelection` drops them from its
 * Enable-all count, with no UI to undo it. Three things keep that a nuisance
 * rather than a lockout, and all three are load-bearing:
 *
 *   - the rows are still rendered, and Settings' own toggle still works: this
 *     changes what the GUI *says*, never what the **daemon** permits;
 *   - the Rust gates are untouched, so nothing here can actually wall a tool;
 *   - the document is fetched over HTTPS from one pinned host, validated in the
 *     main process before it is believed, and the only field read is
 *     `privacy === 'private'`.
 *
 * ⚠ **"Never what it permits" was too strong, and review caught it.** In the
 * *composer's* selector a refused pairing is rendered but `disabled`
 * (`BottomMenuExtensionSelection.tsx` `:249`, `:426`) and is dropped from
 * `toggleableExtensions`, which is the set "Enable all" acts on — so on a
 * public-model chat a key a hostile document taught makes that extension
 * untoggleable from the composer, including untoggleable *off*. The bound on
 * that is `ExtensionItem`'s Switch, which the pairing notice does **not**
 * disable (`isToggling` only), so the extension stays toggleable in Settings and
 * the daemon is unaffected either way. A bounded nuisance with an escape hatch,
 * which is the claim this paragraph is entitled to make — not "no effect".
 *
 * That is the accepted cost of "never lowers": a badge that a hostile document
 * can *add* is strictly better than one it can *remove*, and only the second is
 * a safety failure. There is deliberately no cap and no clear, because both are
 * a lowering mechanism wearing a different name. The set is bounded in practice
 * by the catalogue's size (tens of entries); an unbounded array would eventually
 * hit the storage quota, at which point `rememberPrivateExtensionKeys` holds the
 * key in memory instead of dropping it.
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

/**
 * Keys learned this run that storage REFUSED to take.
 *
 * `setItem` throws on a quota, in some private-browsing modes, and wherever
 * storage is disabled outright. Swallowing that and moving on lost the key
 * twice over — not written, and not held either — so the very next document
 * could take the badge back off, which is the one thing this module exists to
 * prevent. A failed write is a reason to RE-LEARN on the next fetch; it is
 * never a reason to forget now.
 *
 * Held separately from `cachedSet` rather than folded into it because
 * `cachedSet` is keyed on the raw stored string and is therefore invalidated by
 * anything that changes storage; these keys are not in storage, so nothing in
 * storage can speak for them. They live for the life of the module, which is
 * the life of the window — exactly the horizon over which "never lowers" has to
 * hold when the durable half is unavailable.
 */
const unpersisted = new Set<string>();

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
  if (raw !== cachedRaw) {
    cachedRaw = raw;
    cachedSet = parse(raw);
  }
  if (unpersisted.size === 0) return cachedSet;
  return new Set([...cachedSet, ...unpersisted]);
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
  const added: string[] = [];
  for (const key of keys) {
    if (key && !next.has(key)) {
      next.add(key);
      added.push(key);
    }
  }
  if (added.length === 0) return;
  try {
    const raw = JSON.stringify([...next].sort());
    localStorage.setItem(STORAGE_KEY, raw);
    cachedRaw = raw;
    cachedSet = next;
  } catch {
    // Storage refused the write. Keep the badge in memory for the rest of this
    // run and let the next successful fetch persist it — never a reason to fail
    // a load, and never a reason to let the next document lower it.
    for (const key of added) unpersisted.add(key);
  }
}
