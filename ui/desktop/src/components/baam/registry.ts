// Types + loader for the BAAM marketplace registry consumed by the in-app
// Browse Skills / Browse Extensions modals. The registry is published at
// biorouter.ucsf.edu/registry.json (generated from baam.html). We fetch it live
// for freshness and fall back to a snapshot bundled with the app when offline.

import type { ProviderTier } from '../../api/types.gen';
import { nameToKey } from '../settings/extensions/utils';
import { learnedPrivateExtensionKeys, rememberPrivateExtensionKeys } from './privateSet';
import fallback from './registry.fallback.json';
import { PRIVATE_EXTENSION_KEYS } from '../settings/extensions/extensionPrivacy';

export interface RegistryExtension {
  id: string;
  name: string;
  organization: string;
  version: string;
  description: string;
  tags: string[];
  github: string;
  download: string;
  filename: string;
  license?: string;
  /**
   * Registry v2. All three are optional so a v1 document cached before the
   * upgrade still parses — a stale snapshot must degrade, never throw.
   *
   * `extension_name` is the name the installed config entry carries, reduced to
   * its lookup **key**: whitespace stripped and lowercased, so the card's
   * `CDWAgent` is published here as `cdwagent`. That substitution is the
   * contract, not an accident of the generator — treat it as one.
   *
   * The key is always `/^[a-z0-9_-]+$/`. That character set is exactly where
   * `config::extensions::name_to_key` (which `classify_extension` applies) and
   * `agents::extension_manager::normalize` (which the manager applies to the
   * installed config name) provably agree; outside it they diverge, so the
   * generator refuses the name rather than publishing a key the running app
   * would never produce.
   *
   * It exists because `id` is derived from the download filename and agrees with
   * the installed name only by luck (`spokeagent-0.4.1` already diverges), and a
   * suffix-stripping heuristic in a security path is right until it isn't. The
   * generator hard-fails on a private entry without one.
   */
  extension_name?: string;
  /** Absent on a v1 document; the generator emits it for every v2 entry. */
  privacy?: 'private' | 'public';
  /**
   * DR-26. Institution ids from `BaamRegistry.institutions`. Absent means
   * unconstrained — any private model may use it. Never an empty array: the
   * generator rejects one, because "nothing is permitted" and "no constraint"
   * must not share a spelling.
   */
  affiliation?: string[];
}

export type SkillCategory = 'Core' | 'Developer' | 'Biomedical';

export interface RegistrySkill {
  id: string;
  name: string;
  category: SkillCategory;
  type: string;
  description: string;
  tags: string[];
  keywords: string[];
  download: string;
  filename: string;
  license?: string;
}

export interface BaamRegistry {
  version: number;
  source: string;
  /**
   * Registry v2, DR-26. Institution id → display name. Cross-institutional
   * warning copy renders names from here rather than hardcoding them, so an
   * `affiliation` naming an id absent from this map has no name to render with
   * — which is why the generator treats that as a build failure. Optional so a
   * cached v1 document still parses.
   */
  institutions?: Record<string, string>;
  extensions: RegistryExtension[];
  skills: RegistrySkill[];
}

export const FALLBACK_REGISTRY = fallback as BaamRegistry;

/**
 * How long the renderer waits for the main process to answer `registry:fetch`.
 *
 * Deliberately just ABOVE the main process's own 10 s `AbortController` (see
 * `REGISTRY_FETCH_TIMEOUT_MS` in `main.ts`), so a healthy main process always
 * answers first and this budget only ever fires when the IPC channel itself is
 * the thing that is stuck — a wedged handler, a main process busy elsewhere, a
 * window reloaded mid-call. Without it a hung main process hangs the modal
 * forever, showing "Loading catalog…" with no way out.
 */
export const REGISTRY_LOAD_BUDGET_MS = 11_000;

export interface RegistryLoad {
  registry: BaamRegistry;
  /**
   * True only when this document came off a successful fetch just now. A
   * last-good copy and the bundled snapshot are both `false`: the catalogue on
   * screen may be out of date either way, and the user is told so.
   */
  live: boolean;
  /**
   * When the document on screen was fetched, ISO-8601. Absent for the bundled
   * snapshot, which has no fetch to date.
   */
  fetchedAt?: string;
}

/** Resolves to `null` rather than rejecting once the budget is spent. */
function withBudget<T>(promise: Promise<T>): Promise<T | null> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), REGISTRY_LOAD_BUDGET_MS);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      () => {
        clearTimeout(timer);
        resolve(null);
      }
    );
  });
}

/**
 * Load the marketplace registry. Tries the live registry.json published with the
 * website; the main process falls back to its last-good copy and this falls back
 * again to the snapshot bundled with the app, so Browse always works offline.
 *
 * Every document that arrives — live or last-good — teaches the persisted
 * private set (§10.2). That is the "raises" half of the rule; nothing here can
 * lower a badge, because `effectivePrivacy` unions rather than reads.
 */
export async function loadRegistry(): Promise<RegistryLoad> {
  let result: Awaited<ReturnType<typeof window.electron.fetchRegistry>> | null = null;
  try {
    result = await withBudget(window.electron.fetchRegistry());
  } catch {
    // Never rejects. Every caller renders a modal, and a modal that throws on
    // load shows nothing at all — strictly worse than the bundled snapshot.
    result = null;
  }
  if (
    result &&
    'registry' in result &&
    result.registry &&
    Array.isArray(result.registry.skills) &&
    Array.isArray(result.registry.extensions)
  ) {
    rememberPrivateExtensions(result.registry);
    return {
      registry: result.registry,
      live: result.stale !== true,
      fetchedAt: result.fetchedAt,
    };
  }
  return { registry: FALLBACK_REGISTRY, live: false };
}

/**
 * The lookup key an entry claims, or `null` if it claims none.
 *
 * `extension_name` ONLY, and by contract rather than by luck: `id` is derived
 * from the download filename and agrees with the installed name by coincidence
 * (`spokeagent-0.4.1` already diverges), and a suffix-stripping heuristic in a
 * security path is right until it isn't. The generator hard-fails on a private
 * entry without an `extension_name`, so a private entry always has one.
 */
function privacyKeyOf(entry: RegistryExtension): string | null {
  return entry.extension_name ? nameToKey(entry.extension_name) : null;
}

/** Every key this document marks private. */
function privateKeysIn(registry: BaamRegistry): string[] {
  return registry.extensions
    .filter((e) => e.privacy === 'private')
    .map(privacyKeyOf)
    .filter((k): k is string => k !== null);
}

function rememberPrivateExtensions(registry: BaamRegistry): void {
  rememberPrivateExtensionKeys(privateKeysIn(registry));
}

/**
 * **The union rule, as one function rather than four inline ORs** (§10.2):
 *
 * ```
 * private_set = PRIVATE_EXTENSIONS ∪ private(last_good_fetch)
 * ```
 *
 * A live document can RAISE an extension to private and can never lower one.
 * The natural implementation — trusting whatever the fetched document says — is
 * the bug this exists to prevent: a compromised or merely stale `registry.json`
 * would strip the private badge off `ucsfomopagent` on every machine that
 * fetched it, and the badge is the only warning a user gets before pasting a
 * cohort into a public model's chat.
 *
 * The three sources, all of which raise and none of which lower:
 *   1. `PRIVATE_EXTENSION_KEYS` — the compiled-in mirror of the Rust baseline.
 *   2. the persisted last-good set — every private key ever fetched.
 *   3. `registry`, the document in hand.
 */
export function effectivePrivacy(registry: BaamRegistry, name: string): ProviderTier {
  const key = nameToKey(name);
  if (PRIVATE_EXTENSION_KEYS.includes(key)) return 'private';
  if (learnedPrivateExtensionKeys().has(key)) return 'private';
  if (privateKeysIn(registry).includes(key)) return 'private';
  return 'public';
}

/**
 * The marketplace entry for an installed extension, or `null` if the catalogue
 * does not list it.
 *
 * Looser than `privacyKeyOf` on purpose, and only ever consulted for prose. A
 * public entry carries no `extension_name` (the generator emits it for private
 * entries, where the key is load-bearing), so provenance falls back to the id
 * and the display name — `SPOKEAgent` reduces to the installed `spokeagent`
 * even though its id is `spokeagent-0.4.1`. Nothing about the privacy decision
 * goes through here.
 */
export function marketplaceEntryFor(
  registry: BaamRegistry,
  name: string
): RegistryExtension | null {
  const key = nameToKey(name);
  return (
    registry.extensions.find(
      (e) =>
        privacyKeyOf(e) === key || nameToKey(e.id ?? '') === key || nameToKey(e.name ?? '') === key
    ) ?? null
  );
}

/**
 * §13.5's three strings, verbatim, as one total function.
 *
 * Two naming consequences, known rather than discovered:
 *   - a hand-installed extension *named* `ucsfomopagent` inherits the private
 *     badge. Fail-closed, and fine.
 *   - a genuinely private extension renamed locally becomes public. Already the
 *     accepted direction under R11(ii), and unavoidable: the install records no
 *     provenance at all, so the name is the only thing there is to key on.
 */
export function extensionProvenance(registry: BaamRegistry, name: string): string {
  if (effectivePrivacy(registry, name) === 'private') {
    // Reachable only through the union above, every branch of which is a
    // marketplace source — so naming the marketplace here is always true.
    return 'Private — published on the Biorouter marketplace';
  }
  if (marketplaceEntryFor(registry, name)) {
    return 'Public — published on the Biorouter marketplace';
  }
  return 'Public — installed from a file, not on the marketplace. Any model can call it.';
}

/**
 * The staleness line (§10.2), or `null` when the catalogue is fresh — a "this is
 * current" note on every screen is noise, and noise is what makes the stale case
 * invisible.
 */
export function catalogFreshnessLine(load: { live: boolean; fetchedAt?: string }): string | null {
  if (load.live) return null;
  if (!load.fetchedAt) return 'showing bundled catalog (offline)';
  const when = new Date(load.fetchedAt);
  if (Number.isNaN(when.getTime())) return 'showing bundled catalog (offline)';
  return `catalogue last updated ${when.toLocaleDateString()}`;
}

/** Case-insensitive match of a query against a skill's searchable fields. */
export function skillMatches(skill: RegistrySkill, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    skill.name.toLowerCase().includes(needle) ||
    skill.description.toLowerCase().includes(needle) ||
    skill.category.toLowerCase().includes(needle) ||
    skill.tags.some((t) => t.toLowerCase().includes(needle)) ||
    skill.keywords.some((t) => t.toLowerCase().includes(needle)) ||
    (skill.license?.toLowerCase().includes(needle) ?? false)
  );
}

/** Case-insensitive match of a query against an extension's searchable fields. */
export function extensionMatches(ext: RegistryExtension, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    ext.name.toLowerCase().includes(needle) ||
    ext.description.toLowerCase().includes(needle) ||
    ext.organization.toLowerCase().includes(needle) ||
    ext.tags.some((t) => t.toLowerCase().includes(needle)) ||
    (ext.license?.toLowerCase().includes(needle) ?? false)
  );
}
